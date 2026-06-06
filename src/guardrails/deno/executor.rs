use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use deno_core::serde_v8;
use deno_core::v8;
use deno_core::{extension, op2, FastString, JsRuntime, PollEventLoopOptions, RuntimeOptions};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::errors::{AiRestError, ErrorType};
use crate::guardrails::types::GuardrailOutcome;

const BOOTSTRAP: &str = include_str!("../../../guardrails/runtime/bootstrap.js");
const MAX_SCRIPT_BYTES: usize = 51_200;
const DEFAULT_TIMEOUT_MS: u64 = 500;

extension!(airest_guardrail_ext,
    ops = [op_airest_guardrail_log, op_airest_hook_fetch],
);

thread_local! {
    static HOOK_NETWORK_ALLOWLIST: RefCell<Option<crate::hooks::permissions::NetworkAllowlist>> =
        const { RefCell::new(None) };
}

const HOOK_FETCH_MAX_BYTES: usize = 65_536;

#[op2]
#[string]
fn op_airest_hook_fetch(#[string] url: String, #[string] options_json: String) -> String {
    let allowlist = HOOK_NETWORK_ALLOWLIST.with(|slot| slot.borrow().clone());
    let Some(allowlist) = allowlist else {
        return fetch_error("Hook network access is not enabled");
    };
    if !allowlist.is_allowed(&url) {
        return fetch_error("URL is not in the hook network allowlist");
    }

    let options = serde_json::from_str::<serde_json::Value>(&options_json).unwrap_or_default();
    let method = options
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("GET")
        .to_ascii_uppercase();

    if method != "GET" && method != "POST" {
        return fetch_error("Only GET and POST are allowed for host.fetch");
    }

    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(err) => return fetch_error(&err.to_string()),
    };

    let mut request = client.request(
        reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
        &url,
    );
    if let Some(body) = options.get("body").and_then(|b| b.as_str()) {
        request = request.body(body.to_string());
    }
    if let Some(headers) = options.get("headers").and_then(|h| h.as_object()) {
        for (key, item) in headers {
            if let Some(val) = item.as_str() {
                request = request.header(key, val);
            }
        }
    }

    match request.send() {
        Ok(response) => {
            let status = response.status().as_u16();
            let body = response
                .bytes()
                .map(|bytes| bytes.iter().take(HOOK_FETCH_MAX_BYTES).copied().collect::<Vec<_>>())
                .unwrap_or_default();
            let body = String::from_utf8_lossy(&body).to_string();
            serde_json::json!({ "ok": true, "status": status, "body": body }).to_string()
        }
        Err(err) => fetch_error(&err.to_string()),
    }
}

fn fetch_error(message: &str) -> String {
    serde_json::json!({ "ok": false, "error": message }).to_string()
}

#[op2(fast)]
#[string]
fn op_airest_guardrail_log(#[string] level: String, #[string] message: String) {
    match level.as_str() {
        "warn" => tracing::warn!(guardrail = true, "{message}"),
        "error" => tracing::error!(guardrail = true, "{message}"),
        _ => tracing::debug!(guardrail = true, "{message}"),
    }
}

struct DenoJob {
    script_key: String,
    init_script: String,
    eval_source: String,
    network_allowlist: Option<crate::hooks::permissions::NetworkAllowlist>,
    response: oneshot::Sender<Result<GuardrailOutcome, AiRestError>>,
}

/// Dedicated-thread V8 pool — `JsRuntime` is not moved across async tasks.
#[derive(Clone)]
pub struct DenoGuardrailExecutor {
    tx: mpsc::Sender<DenoJob>,
}

impl DenoGuardrailExecutor {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<DenoJob>();
        thread::Builder::new()
            .name("airest-guardrail-v8".into())
            .spawn(move || worker_loop(rx))
            .expect("failed to spawn guardrail V8 worker");

        Self { tx }
    }

    pub async fn evaluate(
        &self,
        script_key: String,
        user_script: &str,
        ctx: &Value,
        timeout_ms: u64,
        network_allowlist: Option<crate::hooks::permissions::NetworkAllowlist>,
    ) -> Result<GuardrailOutcome, AiRestError> {
        if user_script.len() > MAX_SCRIPT_BYTES {
            return Err(AiRestError::new(
                ErrorType::GuardrailViolation,
                "Guardrail script exceeds maximum allowed size.",
            ));
        }

        if network_allowlist.is_none() {
            for forbidden in ["fetch(", "Deno.read", "Deno.write", "require(", "process.", "Deno.run"]
            {
                if user_script.contains(forbidden) {
                    return Err(AiRestError::with_details(
                        ErrorType::GuardrailViolation,
                        "Guardrail script uses forbidden sandbox operation.",
                        serde_json::json!({ "pattern": forbidden }),
                    ));
                }
            }
        }
        // Hook scripts are pre-validated in hooks/deno.rs before wrapping.

        let ctx_json =
            serde_json::to_string(ctx).map_err(|_| internal_error("Failed to serialize context"))?;

        let init_script = format!(
            "{BOOTSTRAP}\n{user_script}\n\
            globalThis.__airestEvaluate = function(ctx) {{\n\
              if (typeof evaluate !== 'function') {{\n\
                throw new Error('Guardrail script must define evaluate(ctx)');\n\
              }}\n\
              return AirestGuardrail.normalizeOutcome(evaluate(ctx));\n\
            }};"
        );

        let eval_source = format!("globalThis.__airestEvaluate({ctx_json})");

        let timeout = Duration::from_millis(timeout_ms.max(1).min(30_000));
        let (response_tx, response_rx) = oneshot::channel();

        self.tx
            .send(DenoJob {
                script_key,
                init_script,
                eval_source,
                network_allowlist,
                response: response_tx,
            })
            .map_err(|_| internal_error("Guardrail V8 worker unavailable"))?;

        match tokio::time::timeout(timeout + Duration::from_millis(50), response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(internal_error("Guardrail worker dropped response")),
            Err(_) => Err(AiRestError::new(
                ErrorType::GuardrailViolation,
                "Guardrail script timed out.",
            )),
        }
    }
}

impl Default for DenoGuardrailExecutor {
    fn default() -> Self {
        Self::new()
    }
}

fn worker_loop(rx: mpsc::Receiver<DenoJob>) {
    let mut runtimes: HashMap<String, JsRuntime> = HashMap::new();

    while let Ok(job) = rx.recv() {
        let DenoJob {
            script_key,
            init_script,
            eval_source,
            network_allowlist,
            response,
        } = job;
        HOOK_NETWORK_ALLOWLIST.with(|slot| *slot.borrow_mut() = network_allowlist);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            evaluate_job(
                &mut runtimes,
                script_key,
                init_script,
                eval_source,
            )
        }));

        let outcome = match result {
            Ok(outcome) => outcome,
            Err(_) => Err(internal_error("Guardrail script panicked")),
        };

        let _ = response.send(outcome);
    }
}

fn evaluate_job(
    runtimes: &mut HashMap<String, JsRuntime>,
    script_key: String,
    init_script: String,
    eval_source: String,
) -> Result<GuardrailOutcome, AiRestError> {
    let runtime = runtimes.entry(script_key).or_insert_with(new_runtime);

    runtime
        .execute_script(
            "airest://guardrail/init",
            FastString::from(init_script),
        )
        .map_err(|e| script_error_display(&e))?;

    let global = runtime
        .execute_script(
            "airest://guardrail/eval",
            FastString::from(eval_source),
        )
        .map_err(|e| script_error_display(&e))?;

    // Sync poll — worker thread owns the isolate.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("guardrail worker tokio runtime");
    rt.block_on(async {
        runtime
            .run_event_loop(PollEventLoopOptions::default())
            .await
            .map_err(|e| script_error_display(&e))?;
        Ok::<(), AiRestError>(())
    })?;

    deno_core::scope!(scope, runtime);
    let local = v8::Local::new(scope, global);
    let value: Value =
        serde_v8::from_v8(scope, local).map_err(|e| script_error_msg(format!("{e:?}")))?;

    parse_outcome_value(value)
}

fn new_runtime() -> JsRuntime {
    JsRuntime::new(RuntimeOptions {
        extensions: vec![airest_guardrail_ext::init()],
        ..Default::default()
    })
}

fn parse_outcome_value(value: Value) -> Result<GuardrailOutcome, AiRestError> {
    let action = value
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| script_error_msg("Guardrail outcome missing action".to_string()))?;

    match action {
        "pass" => Ok(GuardrailOutcome::Pass),
        "warn" => {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| script_error_msg("warn outcome requires message".to_string()))?;
            Ok(GuardrailOutcome::Warn {
                message: message.to_string(),
            })
        }
        "block" => {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| script_error_msg("block outcome requires message".to_string()))?;
            Ok(GuardrailOutcome::Block {
                message: message.to_string(),
                details: value.get("details").cloned(),
            })
        }
        "modify" => Ok(GuardrailOutcome::Modify {
            input: value.get("input").cloned(),
            output: value.get("output").cloned(),
        }),
        other => Err(script_error_msg(format!("Unknown guardrail action: {other}"))),
    }
}

fn script_error_display(err: &impl std::fmt::Display) -> AiRestError {
    script_error_msg(err.to_string())
}

fn script_error_msg(reason: String) -> AiRestError {
    AiRestError::with_details(
        ErrorType::GuardrailViolation,
        "Guardrail script execution failed.",
        serde_json::json!({ "reason": reason }),
    )
}

fn internal_error(message: &str) -> AiRestError {
    AiRestError::new(ErrorType::InternalServer, message)
}

pub fn default_timeout_ms(spec_ms: Option<u64>) -> u64 {
    spec_ms.unwrap_or(DEFAULT_TIMEOUT_MS)
}
