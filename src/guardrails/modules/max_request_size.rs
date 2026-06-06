use serde_json::Value;

use crate::guardrails::types::{GuardrailContext, GuardrailOutcome};

pub fn evaluate(ctx: &GuardrailContext, config: &Value) -> GuardrailOutcome {
    let max_bytes = config
        .get("maxBytes")
        .or_else(|| config.get("maxRequestSize"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1_048_576);

    if ctx.request_body_bytes as u64 > max_bytes {
        return GuardrailOutcome::Block {
            message: format!(
                "Request body exceeds maximum allowed size of {} bytes",
                max_bytes
            ),
            details: Some(serde_json::json!({
                "maxBytes": max_bytes,
                "actualBytes": ctx.request_body_bytes,
            })),
        };
    }

    GuardrailOutcome::Pass
}
