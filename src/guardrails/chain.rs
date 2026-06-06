use std::sync::Arc;

use tracing::{Instrument, warn};

use crate::auth::AuthContext;
use crate::otel::guardrail_module;
use crate::definitions::{EndpointDefinition, GuardrailHook, GuardrailSpec};
use crate::errors::{AiRestError, ErrorType};
use crate::guardrails::builtin::BuiltinGuardrailModule;
use crate::guardrails::context::GuardrailContextPayload;
use crate::guardrails::deno::DenoGuardrailModule;
use crate::guardrails::metrics::GuardrailMetrics;
use crate::guardrails::pluggable::GuardrailModule;
use crate::guardrails::types::{GuardrailContext, GuardrailOutcome};
use crate::guardrails::GuardrailEngine;

/// Compiled guardrail modules for an endpoint (built at YAML load time).
#[derive(Clone)]
pub struct GuardrailChain {
    modules: Vec<(GuardrailHook, Arc<dyn GuardrailModule>)>,
}

impl GuardrailChain {
    pub fn compile(
        specs: &[GuardrailSpec],
        definition_dir: Option<&std::path::Path>,
        engine: &GuardrailEngine,
    ) -> Result<Self, AiRestError> {
        let mut modules = Vec::with_capacity(specs.len());
        for spec in specs {
            let module: Arc<dyn GuardrailModule> = if spec.is_deno() {
                DenoGuardrailModule::from_spec(spec, definition_dir, engine.deno.clone())?
            } else {
                BuiltinGuardrailModule::from_spec(spec)?
            };
            modules.push((spec.hook, module));
        }
        Ok(Self { modules })
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub async fn run_hook<'a>(
        &self,
        hook: GuardrailHook,
        mut ctx: GuardrailContext<'a>,
        auth: Option<&AuthContext>,
        metrics: &mut GuardrailMetrics,
    ) -> Result<GuardrailContext<'a>, AiRestError> {
        for (spec_hook, module) in &self.modules {
            if *spec_hook != hook {
                continue;
            }

            let config = endpoint_guardrail_config(ctx.endpoint, module.name());
            let payload = GuardrailContextPayload::from_runtime(&ctx, &config, auth);

            let span = guardrail_module(
                &ctx.endpoint.name,
                ctx.request_id,
                module.name(),
                module.runtime(),
                hook.as_str(),
            );

            let outcome = async { module.evaluate(&payload).await }
                .instrument(span)
                .await?;
            metrics.record(module.name(), module.runtime(), hook.as_str(), &outcome);

            match outcome {
                GuardrailOutcome::Pass => {}
                GuardrailOutcome::Warn { message } => {
                    warn!(
                        request_id = %ctx.request_id,
                        endpoint = %ctx.endpoint.name,
                        module = module.name(),
                        message = %message,
                        "Guardrail warning"
                    );
                }
                GuardrailOutcome::Modify { input, output } => {
                    if let Some(value) = input {
                        ctx.input = value;
                    }
                    if let Some(value) = output {
                        ctx.output = Some(value);
                    }
                }
                GuardrailOutcome::Block { message, details } => {
                    return Err(match details {
                        Some(details) => AiRestError::with_details(
                            ErrorType::GuardrailViolation,
                            message,
                            details,
                        ),
                        None => AiRestError::new(ErrorType::GuardrailViolation, message),
                    });
                }
            }
        }

        Ok(ctx)
    }
}

fn endpoint_guardrail_config(endpoint: &EndpointDefinition, module_name: &str) -> serde_json::Value {
    endpoint
        .guardrails
        .as_ref()
        .and_then(|specs| {
            specs
                .iter()
                .find(|s| s.module == module_name)
                .map(|s| s.config.clone())
        })
        .unwrap_or(serde_json::json!({}))
}
