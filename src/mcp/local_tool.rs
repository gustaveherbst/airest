use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::definitions::LocalToolSpec;
use crate::errors::{AiRestError, ErrorType};
use crate::guardrails::deno::DenoGuardrailExecutor;
use crate::guardrails::types::GuardrailOutcome;
use crate::hooks::permissions::{
    script_uses_global_fetch, validate_permission_tokens, NetworkAllowlist,
};

static LOCAL_TOOL_EXECUTOR: OnceLock<DenoGuardrailExecutor> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct LocalToolRuntime {
    pub script: String,
    pub permissions: Vec<String>,
    pub timeout_ms: u64,
}

impl LocalToolRuntime {
    pub fn from_spec(spec: &LocalToolSpec) -> Result<Self, AiRestError> {
        let script = spec
            .script
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                AiRestError::new(
                    ErrorType::McpTool,
                    "Local tool script is missing after load.",
                )
            })?
            .to_string();

        let perms = spec.permissions.clone().unwrap_or_default();
        validate_permission_tokens(&perms).map_err(|reason| {
            AiRestError::with_details(
                ErrorType::McpTool,
                "Invalid local tool permissions.",
                json!({ "reason": reason }),
            )
        })?;
        validate_local_tool_sandbox(&script, &NetworkAllowlist::parse(&perms))?;

        Ok(Self {
            script,
            permissions: perms,
            timeout_ms: spec.timeout_ms.unwrap_or(5000),
        })
    }
}

pub async fn execute_local_tool(
    runtime: &LocalToolRuntime,
    tool_name: &str,
    request_id: &str,
    arguments: Value,
) -> Result<Value, AiRestError> {
    let network_allowlist = NetworkAllowlist::parse(&runtime.permissions);
    let ctx = json!({
        "requestId": request_id,
        "tool": tool_name,
        "arguments": arguments,
    });

    let wrapped = wrap_execute_script(&runtime.script, !network_allowlist.is_empty());
    let script_key = format!("local-tool:{tool_name}:{}", hash_script(&wrapped));

    let executor = LOCAL_TOOL_EXECUTOR.get_or_init(DenoGuardrailExecutor::new);
    let outcome = executor
        .evaluate(
            script_key,
            &wrapped,
            &ctx,
            runtime.timeout_ms,
            Some(network_allowlist),
        )
        .await
        .map_err(map_tool_err)?;

    match outcome {
        GuardrailOutcome::Modify { input: Some(value), .. } => Ok(value),
        GuardrailOutcome::Pass => Ok(Value::Object(Default::default())),
        GuardrailOutcome::Block { message, details } => Err(match details {
            Some(details) => AiRestError::with_details(ErrorType::McpTool, message, details),
            None => AiRestError::new(ErrorType::McpTool, message),
        }),
        GuardrailOutcome::Modify { input: None, .. } => Ok(Value::Object(Default::default())),
        GuardrailOutcome::Warn { message } => {
            tracing::warn!(request_id = %request_id, tool = %tool_name, message = %message, "Local tool warning");
            Ok(Value::Object(Default::default()))
        }
    }
}

fn wrap_execute_script(user_script: &str, network_enabled: bool) -> String {
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
  if (typeof execute !== 'function') {{
    throw new Error('Local tool script must define execute(arguments, host)');
  }}
  const host = {{
    requestId: ctx.requestId,
{host_fetch}  }};
  const result = execute(ctx.arguments, host);
  return {{ action: 'modify', input: result }};
}}
"#
    )
}

fn validate_local_tool_sandbox(script: &str, network: &NetworkAllowlist) -> Result<(), AiRestError> {
    for forbidden in ["Deno.read", "Deno.write", "require(", "process.", "Deno.run"] {
        if script.contains(forbidden) {
            return Err(AiRestError::with_details(
                ErrorType::McpTool,
                "Local tool script uses forbidden sandbox operation.",
                json!({ "pattern": forbidden }),
            ));
        }
    }
    if script_uses_global_fetch(script) {
        if network.is_empty() {
            return Err(AiRestError::with_details(
                ErrorType::McpTool,
                "Local tool script uses fetch(); grant network permission to allow host.fetch().",
                json!({ "pattern": "fetch(" }),
            ));
        }
        return Err(AiRestError::with_details(
            ErrorType::McpTool,
            "Use host.fetch() instead of global fetch() in local tool scripts.",
            json!({ "pattern": "fetch(" }),
        ));
    }
    Ok(())
}

fn map_tool_err(err: AiRestError) -> AiRestError {
    if err.error_type() == ErrorType::GuardrailViolation {
        AiRestError::with_details(
            ErrorType::McpTool,
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
