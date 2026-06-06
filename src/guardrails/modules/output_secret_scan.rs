use regex::Regex;
use serde_json::Value;

use crate::guardrails::types::{GuardrailContext, GuardrailOutcome};

const DEFAULT_PATTERNS: &[&str] = &[
    r"AKIA[0-9A-Z]{16}",
    r"sk-[a-zA-Z0-9]{20,}",
    r#"(?i)(api[_-]?key|secret|password|token)\s*[:=]\s*['"]?[a-zA-Z0-9_-]{8,}"#,
];

pub fn evaluate(ctx: &GuardrailContext, config: &Value) -> GuardrailOutcome {
    let patterns: Vec<String> = config
        .get("patterns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_else(|| DEFAULT_PATTERNS.iter().map(|s| s.to_string()).collect());

    let haystack = ctx
        .output
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_default();

    if haystack.is_empty() {
        return GuardrailOutcome::Pass;
    }

    for pattern in &patterns {
        let Ok(re) = Regex::new(pattern) else {
            continue;
        };
        if re.is_match(&haystack) {
            return GuardrailOutcome::Block {
                message: "Output may contain a secret or credential.".to_string(),
                details: Some(serde_json::json!({
                    "pattern": pattern,
                })),
            };
        }
    }

    GuardrailOutcome::Pass
}
