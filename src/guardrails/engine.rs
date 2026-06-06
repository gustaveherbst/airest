use crate::guardrails::deno::DenoGuardrailExecutor;

/// Shared guardrail engine (Deno V8 worker pool for TypeScript modules).
#[derive(Clone)]
pub struct GuardrailEngine {
    pub deno: DenoGuardrailExecutor,
}

impl GuardrailEngine {
    pub fn new() -> Self {
        Self {
            deno: DenoGuardrailExecutor::new(),
        }
    }
}

impl Default for GuardrailEngine {
    fn default() -> Self {
        Self::new()
    }
}
