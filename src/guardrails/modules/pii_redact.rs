use serde_json::{json, Value};

use crate::guardrails::types::{GuardrailContext, GuardrailOutcome};

const REDACTED: &str = "[REDACTED]";

pub fn evaluate(ctx: &GuardrailContext, config: &Value) -> GuardrailOutcome {
    let fields: Vec<String> = config
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if fields.is_empty() {
        return GuardrailOutcome::Pass;
    }

    let mode = config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("mask");

    let mut input = ctx.input.clone();
    if let Some(obj) = input.as_object_mut() {
        for field in &fields {
            if let Some(value) = obj.get_mut(field) {
                match mode {
                    "remove" => {
                        obj.remove(field);
                    }
                    _ => {
                        *value = json!(REDACTED);
                    }
                }
            }
        }
        return GuardrailOutcome::Modify {
            input: Some(Value::Object(obj.clone())),
            output: None,
        };
    }

    GuardrailOutcome::Pass
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::{minimal_test_endpoint, GuardrailHook};

    fn ctx_with_input(input: Value) -> GuardrailContext<'static> {
        static ENDPOINT: std::sync::OnceLock<crate::definitions::EndpointDefinition> =
            std::sync::OnceLock::new();
        let endpoint = ENDPOINT.get_or_init(minimal_test_endpoint);

        GuardrailContext {
            request_id: "req_test",
            endpoint,
            hook: GuardrailHook::PostInput,
            input,
            rendered_system: None,
            rendered_user: None,
            llm_raw: None,
            output: None,
            request_body_bytes: 0,
        }
    }

    #[test]
    fn masks_configured_fields() {
        let config = json!({ "fields": ["email"], "mode": "mask" });
        let input = json!({ "email": "a@b.com", "text": "hello" });
        let ctx = ctx_with_input(input);
        match evaluate(&ctx, &config) {
            GuardrailOutcome::Modify { input: Some(out), .. } => {
                assert_eq!(out["email"], REDACTED);
                assert_eq!(out["text"], "hello");
            }
            other => panic!("expected modify, got {other:?}"),
        }
    }
}
