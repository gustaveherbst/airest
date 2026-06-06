use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::ErrorType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuardrailHook {
    PreInput,
    PostInput,
    PreLlm,
    PostLlm,
    PostOutput,
    PreCacheWrite,
}

impl GuardrailHook {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreInput => "preInput",
            Self::PostInput => "postInput",
            Self::PreLlm => "preLlm",
            Self::PostLlm => "postLlm",
            Self::PostOutput => "postOutput",
            Self::PreCacheWrite => "preCacheWrite",
        }
    }

    pub fn from_str_value(value: &str) -> Option<Self> {
        match value {
            "preInput" => Some(Self::PreInput),
            "postInput" => Some(Self::PostInput),
            "preLlm" => Some(Self::PreLlm),
            "postLlm" => Some(Self::PostLlm),
            "postOutput" => Some(Self::PostOutput),
            "preCacheWrite" => Some(Self::PreCacheWrite),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailSpec {
    pub module: String,
    pub hook: GuardrailHook,
    /// `builtin` (default) or `deno` for TypeScript modules via deno_core.
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Inline TypeScript/JavaScript source (must define `evaluate(ctx)`).
    #[serde(default)]
    pub script: Option<String>,
    /// Path to `.ts`/`.js` file, relative to the endpoint YAML file.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub config: Value,
}

impl GuardrailSpec {
    pub fn builtin(module: &str, hook: GuardrailHook, config: Value) -> Self {
        Self {
            module: module.to_string(),
            hook,
            runtime: Some("builtin".to_string()),
            timeout_ms: None,
            script: None,
            path: None,
            config,
        }
    }

    pub fn deno_script(module: &str, hook: GuardrailHook, script: &str) -> Self {
        Self {
            module: module.to_string(),
            hook,
            runtime: Some("deno".to_string()),
            timeout_ms: Some(2000),
            script: Some(script.to_string()),
            path: None,
            config: Value::Object(Default::default()),
        }
    }

    pub fn is_deno(&self) -> bool {
        matches!(self.runtime.as_deref(), Some("deno"))
            || self.script.is_some()
            || self.path.is_some()
    }

    pub fn is_builtin(&self) -> bool {
        !self.is_deno()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub required: bool,
    /// `apiKey` (default), `jwt`, `oauth2Introspect`, `none`, `trustGateway`
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub jwt: Option<JwtAuthConfig>,
    #[serde(default)]
    pub oauth2: Option<OAuth2IntrospectConfig>,
    #[serde(default)]
    pub trust_gateway: Option<TrustGatewayConfig>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JwtAuthConfig {
    #[serde(default)]
    pub issuer: Option<String>,
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub jwks_url: Option<String>,
    #[serde(default)]
    pub algorithms: Option<Vec<String>>,
    #[serde(default)]
    pub claims: Option<JwtClaimsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct JwtClaimsConfig {
    #[serde(default)]
    pub required: Option<Vec<String>>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2IntrospectConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrustGatewayConfig {
    #[serde(default)]
    pub user_id_header: Option<String>,
    #[serde(default)]
    pub tenant_id_header: Option<String>,
}

impl AuthConfig {
    pub fn auth_type(&self) -> &str {
        self.r#type
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or("apiKey")
    }
}

use crate::llm::ProviderKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
}

impl ModelConfig {
    pub fn provider_kind(&self) -> anyhow::Result<ProviderKind> {
        ProviderKind::parse(&self.provider)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Policies {
    #[serde(default = "default_true")]
    pub validate_input: bool,
    #[serde(default = "default_true")]
    pub validate_output: bool,
    #[serde(default = "default_true")]
    pub retry_on_invalid_json: bool,
    #[serde(default = "default_true")]
    pub retry_on_invalid_schema: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_true")]
    pub strip_markdown_code_fences: bool,
    #[serde(default = "default_true")]
    pub log_requests: bool,
    #[serde(default)]
    pub log_responses: bool,
    #[serde(default)]
    pub redact_inputs: bool,
    #[serde(default)]
    pub redact_outputs: bool,
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: u32,
    #[serde(default = "default_tool_timeout_ms")]
    pub tool_timeout_ms: u64,
}

fn default_max_tool_rounds() -> u32 {
    5
}

fn default_tool_timeout_ms() -> u64 {
    10_000
}

/// Overrides the default aiREST error message (and optionally type/status) for a given failure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorOverride {
    pub message: String,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
}

/// Per-endpoint REST error overrides. When present, replaces the standard framework message.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointErrors {
    #[serde(default)]
    pub input_validation: Option<ErrorOverride>,
    #[serde(default)]
    pub authentication: Option<ErrorOverride>,
    #[serde(default)]
    pub prompt_rendering: Option<ErrorOverride>,
    #[serde(default)]
    pub model_provider: Option<ErrorOverride>,
    #[serde(default)]
    pub model_json_parse: Option<ErrorOverride>,
    #[serde(default)]
    pub model_output_validation: Option<ErrorOverride>,
    #[serde(default)]
    pub internal_server: Option<ErrorOverride>,
    #[serde(default)]
    pub guardrail: Option<ErrorOverride>,
    #[serde(default)]
    pub hook_execution: Option<ErrorOverride>,
    #[serde(default)]
    pub cache: Option<ErrorOverride>,
}

impl EndpointErrors {
    pub fn for_type(&self, error_type: ErrorType) -> Option<&ErrorOverride> {
        match error_type {
            ErrorType::InputValidation => self.input_validation.as_ref(),
            ErrorType::Authentication => self.authentication.as_ref(),
            ErrorType::PromptRendering => self.prompt_rendering.as_ref(),
            ErrorType::ModelProvider => self.model_provider.as_ref(),
            ErrorType::ModelJsonParse => self.model_json_parse.as_ref(),
            ErrorType::ModelOutputValidation => self.model_output_validation.as_ref(),
            ErrorType::InternalServer => self.internal_server.as_ref(),
            ErrorType::GuardrailViolation => self.guardrail.as_ref(),
            ErrorType::HookExecution => self.hook_execution.as_ref(),
            ErrorType::Cache => self.cache.as_ref(),
            ErrorType::EndpointDefinition | ErrorType::NotFound | ErrorType::McpTool => None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CacheEmbedderConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CacheStoreConfig {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CacheConfig {
    pub enabled: bool,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub similarity_threshold: Option<f64>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    #[serde(default)]
    pub max_entries: Option<usize>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub exclude_fields: Option<Vec<String>>,
    #[serde(default)]
    pub embedder: Option<CacheEmbedderConfig>,
    #[serde(default)]
    pub store: Option<CacheStoreConfig>,
    #[serde(default)]
    pub bypass_on_guardrail_block: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HookSpec {
    pub runtime: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
    pub script: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointHooks {
    #[serde(default)]
    pub pre_request: Option<HookSpec>,
    #[serde(default)]
    pub post_input: Option<HookSpec>,
    #[serde(default)]
    pub pre_llm: Option<HookSpec>,
    #[serde(default)]
    pub post_output: Option<HookSpec>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub transport: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolsConfig {
    #[serde(default)]
    pub mcp_servers: Option<Vec<McpServerConfig>>,
    #[serde(default)]
    pub local: Option<Vec<LocalToolSpec>>,
    #[serde(default)]
    pub allow: Option<Vec<String>>,
    #[serde(default)]
    pub tool_timeout_ms: Option<u64>,
}

/// In-process Deno tool (no MCP server). Invoked by the model via the same tool loop as MCP tools.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolSpec {
    pub name: String,
    pub description: String,
    /// Extra guidance appended to `description` for the LLM tool schema.
    #[serde(default)]
    pub tool_prompt: Option<String>,
    pub input_schema: Value,
    #[serde(default = "default_deno_runtime")]
    pub runtime: String,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn default_deno_runtime() -> String {
    "deno".to_string()
}

impl LocalToolSpec {
    pub fn llm_description(&self) -> String {
        match &self.tool_prompt {
            Some(prompt) if !prompt.trim().is_empty() => {
                format!("{}\n\n{}", self.description.trim(), prompt.trim())
            }
            _ => self.description.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointExamples {
    #[serde(default)]
    pub request: Option<Value>,
    #[serde(default)]
    pub response: Option<Value>,
}

/// Optional per-endpoint health check configuration for GET {path}/health.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HealthConfig {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub status: Option<u16>,
}

fn default_true() -> bool {
    true
}

fn default_max_retries() -> u32 {
    2
}

impl Default for Policies {
    fn default() -> Self {
        Self {
            validate_input: true,
            validate_output: true,
            retry_on_invalid_json: true,
            retry_on_invalid_schema: true,
            max_retries: 2,
            strip_markdown_code_fences: true,
            log_requests: true,
            log_responses: false,
            redact_inputs: false,
            redact_outputs: false,
            max_tool_rounds: 5,
            tool_timeout_ms: 10_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointDefinition {
    /// Unique aiREST API identifier (independent of the definition filename).
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Optional grouping label for cataloging related aiREST APIs (e.g. legal, support).
    #[serde(default)]
    pub category: Option<String>,
    pub method: String,
    /// REST route exposed by this aiREST API.
    pub path: String,
    #[serde(default)]
    pub auth: Option<AuthConfig>,
    pub input_schema: Value,
    pub system_prompt: String,
    #[serde(default)]
    pub user_prompt_template: Option<String>,
    pub output_schema: Value,
    pub model: ModelConfig,
    #[serde(default)]
    pub policies: Policies,
    #[serde(default)]
    pub errors: Option<EndpointErrors>,
    #[serde(default)]
    pub examples: Option<EndpointExamples>,
    #[serde(default)]
    pub health: Option<HealthConfig>,
    #[serde(default)]
    pub guardrails: Option<Vec<GuardrailSpec>>,
    #[serde(default)]
    pub cache: Option<CacheConfig>,
    #[serde(default)]
    pub telemetry: Option<TelemetryConfig>,
    #[serde(default)]
    pub hooks: Option<EndpointHooks>,
    #[serde(default)]
    pub tools: Option<ToolsConfig>,
}

impl EndpointDefinition {
    pub fn auth_required(&self) -> bool {
        self.auth.as_ref().is_some_and(|a| a.required)
    }

    pub fn policies(&self) -> &Policies {
        &self.policies
    }

    pub fn health_path(&self) -> String {
        format!("{}/health", self.path.trim_end_matches('/'))
    }

    pub fn display_label(&self) -> String {
        match &self.category {
            Some(category) => format!("{category}/{}", self.name),
            None => self.name.clone(),
        }
    }

    pub fn http_method(&self) -> &str {
        self.method.trim()
    }

    pub fn is_get(&self) -> bool {
        self.method.eq_ignore_ascii_case("GET")
    }

    pub fn is_post(&self) -> bool {
        self.method.eq_ignore_ascii_case("POST")
    }

    pub fn route_key(&self) -> String {
        format!("{}:{}", self.method.to_ascii_uppercase(), self.path)
    }
}

/// Minimal endpoint for unit tests — override fields as needed.
pub fn minimal_test_endpoint() -> EndpointDefinition {
    EndpointDefinition {
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        description: None,
        category: None,
        method: "POST".to_string(),
        path: "/v1/test".to_string(),
        auth: None,
        input_schema: serde_json::json!({"type":"object"}),
        system_prompt: "test".to_string(),
        user_prompt_template: None,
        output_schema: serde_json::json!({"type":"object"}),
        model: ModelConfig {
            provider: "openai".to_string(),
            model: "gpt-4.1-mini".to_string(),
            temperature: None,
            max_tokens: None,
        },
        policies: Policies::default(),
        errors: None,
        examples: None,
        health: None,
        guardrails: None,
        cache: None,
        telemetry: None,
        hooks: None,
        tools: None,
    }
}
