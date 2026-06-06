use async_trait::async_trait;

use crate::errors::AiRestError;
use crate::guardrails::context::GuardrailContextPayload;
use crate::guardrails::types::GuardrailOutcome;

/// Pluggable guardrail module — implemented by Rust builtins and Deno (TypeScript) modules.
#[async_trait]
pub trait GuardrailModule: Send + Sync {
    fn name(&self) -> &str;
    fn runtime(&self) -> &'static str;
    async fn evaluate(
        &self,
        ctx: &GuardrailContextPayload,
    ) -> Result<GuardrailOutcome, AiRestError>;
}
