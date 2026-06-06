use serde_json::Value;

use crate::guardrails::types::{GuardrailContext, GuardrailOutcome};

pub fn evaluate(ctx: &GuardrailContext, config: &Value) -> GuardrailOutcome {
    let allowed: Vec<String> = config
        .get("allowedTopics")
        .or_else(|| config.get("allowed_topics"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if allowed.is_empty() {
        return GuardrailOutcome::Pass;
    }

    let field = config
        .get("field")
        .and_then(|v| v.as_str())
        .unwrap_or("text");

    let haystack = ctx
        .input
        .get(field)
        .and_then(|v| v.as_str())
        .map(|text| text.to_ascii_lowercase())
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| ctx.input.to_string().to_ascii_lowercase());

    for topic in &allowed {
        if haystack.contains(&topic.to_ascii_lowercase()) {
            return GuardrailOutcome::Pass;
        }
    }

    GuardrailOutcome::Block {
        message: "Input does not match any allowed topic.".to_string(),
        details: Some(serde_json::json!({
            "allowedTopics": allowed,
            "field": field,
        })),
    }
}
