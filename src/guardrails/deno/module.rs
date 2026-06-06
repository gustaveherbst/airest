use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::definitions::GuardrailSpec;
use crate::errors::{AiRestError, ErrorType};
use crate::guardrails::context::GuardrailContextPayload;
use crate::guardrails::deno::executor::{default_timeout_ms, DenoGuardrailExecutor};
use crate::guardrails::pluggable::GuardrailModule;
use crate::guardrails::types::GuardrailOutcome;

pub struct DenoGuardrailModule {
    pub name: String,
    script_key: String,
    user_script: String,
    timeout_ms: u64,
    executor: DenoGuardrailExecutor,
}

impl DenoGuardrailModule {
    pub fn from_spec(
        spec: &GuardrailSpec,
        definition_dir: Option<&Path>,
        executor: DenoGuardrailExecutor,
    ) -> Result<Arc<dyn GuardrailModule>, AiRestError> {
        let user_script = resolve_script(spec, definition_dir)?;
        validate_script_safety(&user_script)?;
        let script_key = format!("{}:{}", spec.module, hash_script(&user_script));

        Ok(Arc::new(Self {
            name: spec.module.clone(),
            script_key,
            user_script,
            timeout_ms: default_timeout_ms(spec.timeout_ms),
            executor,
        }))
    }
}

fn resolve_script(spec: &GuardrailSpec, definition_dir: Option<&Path>) -> Result<String, AiRestError> {
    if let Some(script) = &spec.script {
        if !script.trim().is_empty() {
            return crate::script::prepare_deno_script(script, None);
        }
    }

    if let Some(path) = &spec.path {
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            let base = definition_dir.ok_or_else(|| {
                AiRestError::new(
                    ErrorType::EndpointDefinition,
                    "Guardrail path is relative but definition file path is unknown.",
                )
            })?;
            base.join(path)
        };

        let raw = std::fs::read_to_string(&resolved).map_err(|e| {
            AiRestError::with_details(
                ErrorType::EndpointDefinition,
                "Failed to read guardrail script file.",
                serde_json::json!({
                    "path": resolved.display().to_string(),
                    "reason": e.to_string(),
                }),
            )
        })?;

        return crate::script::prepare_deno_script(&raw, Some(resolved.as_path()));
    }

    Err(AiRestError::new(
        ErrorType::EndpointDefinition,
        "Deno guardrail requires script or path.",
    ))
}

fn validate_script_safety(script: &str) -> Result<(), AiRestError> {
    const MAX_SCRIPT_BYTES: usize = 51_200;
    if script.len() > MAX_SCRIPT_BYTES {
        return Err(AiRestError::new(
            ErrorType::GuardrailViolation,
            "Guardrail script exceeds maximum allowed size.",
        ));
    }
    for forbidden in ["fetch(", "Deno.read", "Deno.write", "require(", "process.", "Deno.run"] {
        if script.contains(forbidden) {
            return Err(AiRestError::with_details(
                ErrorType::GuardrailViolation,
                "Guardrail script uses forbidden sandbox operation.",
                serde_json::json!({ "pattern": forbidden }),
            ));
        }
    }
    Ok(())
}

fn hash_script(script: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(script.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[async_trait]
impl GuardrailModule for DenoGuardrailModule {
    fn name(&self) -> &str {
        &self.name
    }

    fn runtime(&self) -> &'static str {
        "deno"
    }

    async fn evaluate(
        &self,
        ctx: &GuardrailContextPayload,
    ) -> Result<GuardrailOutcome, AiRestError> {
        let ctx_value = serde_json::to_value(ctx).map_err(|_| {
            AiRestError::new(
                ErrorType::InternalServer,
                "Failed to serialize guardrail context.",
            )
        })?;

        self.executor
            .evaluate(
                self.script_key.clone(),
                &self.user_script,
                &ctx_value,
                self.timeout_ms,
                None,
            )
            .await
    }
}
