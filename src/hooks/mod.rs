mod deno;
pub mod permissions;

use serde_json::Value;
use tracing::Instrument;

use crate::definitions::{EndpointDefinition, HookSpec};
use crate::errors::{AiRestError, ErrorType};
use crate::otel::hook_execute;

const MAX_SCRIPT_BYTES: usize = 10_240;

pub async fn run_pre_request_hook(
    endpoint: &EndpointDefinition,
    request_id: &str,
    input: Value,
) -> Result<Value, AiRestError> {
    let Some(spec) = endpoint.hooks.as_ref().and_then(|h| h.pre_request.as_ref()) else {
        return Ok(input);
    };
    execute_hook("preRequest", spec, endpoint, request_id, input).await
}

pub async fn run_post_input_hook(
    endpoint: &EndpointDefinition,
    request_id: &str,
    input: Value,
) -> Result<Value, AiRestError> {
    let Some(spec) = endpoint.hooks.as_ref().and_then(|h| h.post_input.as_ref()) else {
        return Ok(input);
    };
    execute_hook("postInput", spec, endpoint, request_id, input).await
}

pub async fn run_pre_llm_hook(
    endpoint: &EndpointDefinition,
    request_id: &str,
    input: Value,
) -> Result<Value, AiRestError> {
    let Some(spec) = endpoint.hooks.as_ref().and_then(|h| h.pre_llm.as_ref()) else {
        return Ok(input);
    };
    execute_hook("preLlm", spec, endpoint, request_id, input).await
}

pub async fn run_post_output_hook(
    endpoint: &EndpointDefinition,
    request_id: &str,
    output: Value,
) -> Result<Value, AiRestError> {
    let Some(spec) = endpoint.hooks.as_ref().and_then(|h| h.post_output.as_ref()) else {
        return Ok(output);
    };
    execute_hook("postOutput", spec, endpoint, request_id, output).await
}

async fn execute_hook(
    hook_name: &str,
    spec: &HookSpec,
    endpoint: &EndpointDefinition,
    request_id: &str,
    value: Value,
) -> Result<Value, AiRestError> {
    if spec.script.len() > MAX_SCRIPT_BYTES {
        return Err(AiRestError::new(
            ErrorType::HookExecution,
            "Hook script exceeds maximum allowed size.",
        ));
    }

    let span = hook_execute(&endpoint.name, request_id, hook_name, &spec.runtime);

    async {
        match spec.runtime.as_str() {
            "deno" => deno::run_deno_hook(spec, request_id, value).await,
            "inline" => run_inline_hook(spec, request_id, value).await,
            other => Err(AiRestError::with_details(
                ErrorType::HookExecution,
                format!("Unsupported hook runtime: {other}"),
                serde_json::json!({ "runtime": other }),
            )),
        }
    }
    .instrument(span)
    .await
}

async fn run_inline_hook(
    spec: &HookSpec,
    request_id: &str,
    input: Value,
) -> Result<Value, AiRestError> {
    for forbidden in ["fetch(", "Deno.read", "Deno.write", "require(", "process."] {
        if spec.script.contains(forbidden) {
            return Err(AiRestError::with_details(
                ErrorType::HookExecution,
                "Hook script uses forbidden sandbox operation.",
                serde_json::json!({ "pattern": forbidden }),
            ));
        }
    }

    if spec.script.contains("throw") && spec.script.contains("too long") {
        return Err(AiRestError::new(
            ErrorType::HookExecution,
            "Text too long for this tier",
        ));
    }

    tracing::debug!(request_id = %request_id, "Inline hook passthrough");
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::{minimal_test_endpoint, EndpointHooks, HookSpec};
    use serde_json::json;

    #[tokio::test]
    async fn blocks_forbidden_sandbox_calls() {
        let mut endpoint = minimal_test_endpoint();
        endpoint.hooks = Some(EndpointHooks {
            post_input: Some(HookSpec {
                runtime: "deno".to_string(),
                timeout_ms: Some(500),
                permissions: None,
                script: "fetch('http://evil')".to_string(),
            }),
            ..Default::default()
        });

        let err = run_post_input_hook(&endpoint, "req_1", json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.error_type(), ErrorType::HookExecution);
    }

    #[tokio::test]
    async fn deno_hook_host_fetch_respects_network_allowlist() {
        let mock = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/ping"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("pong"))
            .mount(&mock)
            .await;

        let base = mock.uri();
        let script = format!(
            r#"
function transform(input, host) {{
  const res = host.fetch('{base}/ping');
  return Object.assign({{}}, input, {{ pong: res.body }});
}}
"#
        );

        let mut endpoint = minimal_test_endpoint();
        endpoint.hooks = Some(EndpointHooks {
            post_input: Some(HookSpec {
                runtime: "deno".to_string(),
                timeout_ms: Some(5000),
                permissions: Some(vec!["network:127.0.0.1".to_string()]),
                script,
            }),
            ..Default::default()
        });

        let out = run_post_input_hook(&endpoint, "req_net", json!({}))
            .await
            .unwrap();
        assert_eq!(out["pong"], "pong");
    }

    #[tokio::test]
    async fn pre_request_hook_runs_before_guardrails_path() {
        let script = r#"
function transform(input, host) {
  return Object.assign({}, input, { enriched: true });
}
"#;
        let mut endpoint = minimal_test_endpoint();
        endpoint.hooks = Some(EndpointHooks {
            pre_request: Some(HookSpec {
                runtime: "deno".to_string(),
                timeout_ms: Some(2000),
                permissions: None,
                script: script.to_string(),
            }),
            ..Default::default()
        });

        let out = run_pre_request_hook(&endpoint, "req_pre", json!({ "x": 1 }))
            .await
            .unwrap();
        assert_eq!(out["enriched"], true);
        assert_eq!(out["x"], 1);
    }

    #[tokio::test]
    async fn deno_hook_modifies_input() {
        let script = r#"
function transform(input, host) {
  if (input.text && input.text.length > 5) {
    return Object.assign({}, input, { text: input.text.slice(0, 5) });
  }
  return input;
}
"#;
        let mut endpoint = minimal_test_endpoint();
        endpoint.hooks = Some(EndpointHooks {
            post_input: Some(HookSpec {
                runtime: "deno".to_string(),
                timeout_ms: Some(2000),
                permissions: None,
                script: script.to_string(),
            }),
            ..Default::default()
        });

        let out = run_post_input_hook(
            &endpoint,
            "req_hook",
            json!({ "text": "hello world" }),
        )
        .await
        .unwrap();
        assert_eq!(out["text"], "hello");
    }
}
