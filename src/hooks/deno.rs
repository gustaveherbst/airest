use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::definitions::HookSpec;
use crate::errors::{AiRestError, ErrorType};
use crate::guardrails::deno::DenoGuardrailExecutor;
use crate::guardrails::types::GuardrailOutcome;
use crate::hooks::permissions::{
    script_uses_global_fetch, validate_permission_tokens, NetworkAllowlist,
};

static HOOK_EXECUTOR: OnceLock<DenoGuardrailExecutor> = OnceLock::new();

pub async fn run_deno_hook(
    spec: &HookSpec,
    request_id: &str,
    input: Value,
) -> Result<Value, AiRestError> {
    let network_allowlist = network_allowlist_for(spec)?;
    validate_hook_sandbox(&spec.script, &network_allowlist)?;

    let timeout_ms = spec.timeout_ms.unwrap_or(500);
    let ctx = json!({
        "requestId": request_id,
        "input": input,
    });

    let wrapped = wrap_transform_script(&spec.script, !network_allowlist.is_empty());
    let script_key = format!("hook:{}", hash_script(&wrapped));

    let executor = HOOK_EXECUTOR.get_or_init(DenoGuardrailExecutor::new);
    let outcome = executor
        .evaluate(
            script_key,
            &wrapped,
            &ctx,
            timeout_ms,
            Some(network_allowlist),
        )
        .await
        .map_err(map_guardrail_err)?;

    match outcome {
        GuardrailOutcome::Pass => Ok(ctx
            .get("input")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()))),
        GuardrailOutcome::Modify { input: Some(value), .. } => Ok(value),
        GuardrailOutcome::Modify { input: None, .. } => Ok(ctx
            .get("input")
            .cloned()
            .unwrap_or_else(|| Value::Object(Default::default()))),
        GuardrailOutcome::Block { message, details } => Err(match details {
            Some(details) => AiRestError::with_details(ErrorType::HookExecution, message, details),
            None => AiRestError::new(ErrorType::HookExecution, message),
        }),
        GuardrailOutcome::Warn { message } => {
            tracing::warn!(request_id = %request_id, message = %message, "Hook warning");
            Ok(ctx
                .get("input")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default())))
        }
    }
}

fn network_allowlist_for(spec: &HookSpec) -> Result<NetworkAllowlist, AiRestError> {
    let perms = spec.permissions.as_deref().unwrap_or(&[]);
    validate_permission_tokens(perms).map_err(|reason| {
        AiRestError::with_details(
            ErrorType::HookExecution,
            "Invalid hook permissions.",
            json!({ "reason": reason }),
        )
    })?;
    Ok(NetworkAllowlist::parse(perms))
}

fn wrap_transform_script(user_script: &str, network_enabled: bool) -> String {
    let host_fetch = if network_enabled {
        r#"
  fetch(url, options) {
    const raw = Deno.core.ops.op_airest_hook_fetch(String(url), JSON.stringify(options ?? {}));
    const parsed = JSON.parse(raw);
    if (!parsed.ok) throw new Error(parsed.error);
    return parsed;
  },
"#
    } else {
        ""
    };

    format!(
        r#"{user_script}

function evaluate(ctx) {{
  if (typeof transform !== 'function') {{
    throw new Error('Hook script must define transform(input, host)');
  }}
  const host = {{
    requestId: ctx.requestId,
{host_fetch}  }};
  const result = transform(ctx.input, host);
  return {{ action: 'modify', input: result }};
}}
"#
    )
}

fn validate_hook_sandbox(script: &str, network: &NetworkAllowlist) -> Result<(), AiRestError> {
    for forbidden in ["Deno.read", "Deno.write", "require(", "process.", "Deno.run"] {
        if script.contains(forbidden) {
            return Err(AiRestError::with_details(
                ErrorType::HookExecution,
                "Hook script uses forbidden sandbox operation.",
                json!({ "pattern": forbidden }),
            ));
        }
    }
    if script_uses_global_fetch(script) {
        if network.is_empty() {
            return Err(AiRestError::with_details(
                ErrorType::HookExecution,
                "Hook script uses fetch(); grant network permission to allow host.fetch().",
                json!({ "pattern": "fetch(" }),
            ));
        }
        return Err(AiRestError::with_details(
            ErrorType::HookExecution,
            "Use host.fetch() instead of global fetch() in hook scripts.",
            json!({ "pattern": "fetch(" }),
        ));
    }
    Ok(())
}

fn map_guardrail_err(err: AiRestError) -> AiRestError {
    if err.error_type() == ErrorType::GuardrailViolation {
        AiRestError::with_details(
            ErrorType::HookExecution,
            err.message(),
            err.details().cloned().unwrap_or_else(|| json!({})),
        )
    } else {
        err
    }
}

fn hash_script(script: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(script.as_bytes());
    format!("{:x}", digest)[..16].to_string()
}
