use regex::Regex;
use serde_json::Value;

use crate::guardrails::types::{GuardrailContext, GuardrailOutcome};

pub fn evaluate(ctx: &GuardrailContext, config: &Value) -> GuardrailOutcome {
    let patterns: Vec<String> = config
        .get("patterns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if patterns.is_empty() {
        return GuardrailOutcome::Pass;
    }

    let target = config
        .get("target")
        .and_then(|v| v.as_str())
        .unwrap_or("input");

    let haystack = match target {
        "prompt" | "system" => ctx.rendered_system.unwrap_or("").to_string(),
        "user" => ctx.rendered_user.unwrap_or("").to_string(),
        "llm" | "response" => ctx.llm_raw.unwrap_or("").to_string(),
        "output" => ctx
            .output
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default(),
        _ => ctx.input.to_string(),
    };

    for pattern in &patterns {
        let Ok(re) = Regex::new(pattern) else {
            continue;
        };
        if re.is_match(&haystack) {
            return GuardrailOutcome::Block {
                message: format!("Content matched blocked pattern: {pattern}"),
                details: Some(serde_json::json!({
                    "pattern": pattern,
                    "target": target,
                })),
            };
        }
    }

    GuardrailOutcome::Pass
}
