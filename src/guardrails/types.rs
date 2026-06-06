use serde_json::Value;

use crate::definitions::{EndpointDefinition, GuardrailHook};

/// Mutable execution context passed through the guardrail chain.
#[derive(Debug, Clone)]
pub struct GuardrailContext<'a> {
    pub request_id: &'a str,
    pub endpoint: &'a EndpointDefinition,
    pub hook: GuardrailHook,
    pub input: Value,
    pub rendered_system: Option<&'a str>,
    pub rendered_user: Option<&'a str>,
    pub llm_raw: Option<&'a str>,
    pub output: Option<Value>,
    pub request_body_bytes: usize,
}

#[derive(Debug)]
pub enum GuardrailOutcome {
    Pass,
    Block {
        message: String,
        details: Option<Value>,
    },
    Modify {
        input: Option<Value>,
        output: Option<Value>,
    },
    Warn {
        message: String,
    },
}
