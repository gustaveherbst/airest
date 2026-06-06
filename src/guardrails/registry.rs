use crate::auth::AuthContext;
use crate::definitions::{EndpointDefinition, GuardrailHook};
use crate::errors::AiRestError;
use crate::guardrails::chain::GuardrailChain;
use crate::guardrails::metrics::GuardrailMetrics;
use crate::guardrails::types::GuardrailContext;
use crate::otel::TelemetryState;

pub async fn run_hook<'a>(
    chain: Option<&GuardrailChain>,
    endpoint: &'a EndpointDefinition,
    hook: GuardrailHook,
    ctx: GuardrailContext<'a>,
    auth: Option<&AuthContext>,
    telemetry: Option<&TelemetryState>,
) -> Result<GuardrailContext<'a>, AiRestError> {
    let Some(chain) = chain else {
        return Ok(ctx);
    };

    let mut metrics = GuardrailMetrics::default();
    let result = chain.run_hook(hook, ctx, auth, &mut metrics).await;
    if let Some(telemetry) = telemetry {
        if telemetry.endpoint_enabled(endpoint) {
            telemetry.record_guardrail_metrics(&endpoint.name, &metrics);
        }
    }
    result
}
