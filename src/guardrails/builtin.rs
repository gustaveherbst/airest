use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::definitions::GuardrailSpec;
use crate::errors::AiRestError;
use crate::guardrails::context::GuardrailContextPayload;
use crate::guardrails::modules::{
    max_request_size, output_secret_scan, pii_redact, regex_block, topic_allowlist,
};
use crate::guardrails::pluggable::GuardrailModule;
use crate::guardrails::types::{GuardrailContext, GuardrailOutcome};

pub struct BuiltinGuardrailModule {
    pub name: String,
    pub config: Value,
    kind: BuiltinKind,
}

enum BuiltinKind {
    MaxRequestSize,
    PiiRedact,
    RegexBlock,
    TopicAllowlist,
    OutputSecretScan,
}

impl BuiltinGuardrailModule {
    pub fn from_spec(spec: &GuardrailSpec) -> Result<Arc<dyn GuardrailModule>, AiRestError> {
        let kind = match spec.module.as_str() {
            "max-request-size" | "maxRequestSize" => BuiltinKind::MaxRequestSize,
            "pii-redact" | "piiRedact" => BuiltinKind::PiiRedact,
            "regex-block" | "regexBlock" => BuiltinKind::RegexBlock,
            "topic-allowlist" | "topicAllowlist" => BuiltinKind::TopicAllowlist,
            "output-secret-scan" | "outputSecretScan" => BuiltinKind::OutputSecretScan,
            other => {
                return Err(AiRestError::with_details(
                    crate::errors::ErrorType::EndpointDefinition,
                    format!("Unknown built-in guardrail module: {other}"),
                    serde_json::json!({ "module": other }),
                ));
            }
        };

        Ok(Arc::new(Self {
            name: spec.module.clone(),
            config: spec.config.clone(),
            kind,
        }))
    }
}

#[async_trait]
impl GuardrailModule for BuiltinGuardrailModule {
    fn name(&self) -> &str {
        &self.name
    }

    fn runtime(&self) -> &'static str {
        "builtin"
    }

    async fn evaluate(
        &self,
        payload: &GuardrailContextPayload,
    ) -> Result<GuardrailOutcome, AiRestError> {
        let ctx = runtime_context_from_payload(payload);
        let outcome = match self.kind {
            BuiltinKind::MaxRequestSize => max_request_size::evaluate(&ctx, &self.config),
            BuiltinKind::PiiRedact => pii_redact::evaluate(&ctx, &self.config),
            BuiltinKind::RegexBlock => regex_block::evaluate(&ctx, &self.config),
            BuiltinKind::TopicAllowlist => topic_allowlist::evaluate(&ctx, &self.config),
            BuiltinKind::OutputSecretScan => output_secret_scan::evaluate(&ctx, &self.config),
        };
        Ok(outcome)
    }
}

fn runtime_context_from_payload<'a>(payload: &'a GuardrailContextPayload) -> GuardrailContext<'a> {
    use crate::definitions::minimal_test_endpoint;
    use crate::definitions::GuardrailHook;

    // Builtins only use fields on GuardrailContext; endpoint ref is for metadata/logging.
    static PLACEHOLDER: std::sync::OnceLock<crate::definitions::EndpointDefinition> =
        std::sync::OnceLock::new();
    let endpoint = PLACEHOLDER.get_or_init(minimal_test_endpoint);

    GuardrailContext {
        request_id: &payload.request_id,
        endpoint,
        hook: GuardrailHook::from_str_value(&payload.hook).unwrap_or(GuardrailHook::PostInput),
        input: payload.input.clone(),
        rendered_system: payload.rendered_system.as_deref(),
        rendered_user: payload.rendered_user.as_deref(),
        llm_raw: payload.llm_raw.as_deref(),
        output: payload.output.clone(),
        request_body_bytes: payload.request_body_bytes,
    }
}
