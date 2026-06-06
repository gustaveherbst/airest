use serde::Serialize;
use serde_json::Value;

use crate::auth::AuthContext;
use crate::guardrails::types::GuardrailContext;

/// JSON-serializable snapshot passed to TypeScript `evaluate(ctx)` and Rust builtins.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailContextPayload {
    pub request_id: String,
    pub endpoint: String,
    pub version: String,
    pub hook: String,
    pub input: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    pub request_body_bytes: usize,
    pub config: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthContextPayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthContextPayload {
    pub subject: Option<String>,
    pub tenant_id: Option<String>,
    pub scopes: Vec<String>,
}

impl GuardrailContextPayload {
    pub fn from_runtime<'a>(
        ctx: &GuardrailContext<'a>,
        config: &Value,
        auth: Option<&AuthContext>,
    ) -> Self {
        Self {
            request_id: ctx.request_id.to_string(),
            endpoint: ctx.endpoint.name.clone(),
            version: ctx.endpoint.version.clone(),
            hook: ctx.hook.as_str().to_string(),
            input: ctx.input.clone(),
            rendered_system: ctx.rendered_system.map(str::to_string),
            rendered_user: ctx.rendered_user.map(str::to_string),
            llm_raw: ctx.llm_raw.map(str::to_string),
            output: ctx.output.clone(),
            request_body_bytes: ctx.request_body_bytes,
            config: config.clone(),
            auth: auth.map(|a| AuthContextPayload {
                subject: a.subject.clone(),
                tenant_id: a.tenant_id.clone(),
                scopes: a.scopes.clone(),
            }),
        }
    }
}
