use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use thiserror::Error;

use crate::definitions::EndpointDefinition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    EndpointDefinition,
    InputValidation,
    Authentication,
    PromptRendering,
    ModelProvider,
    ModelJsonParse,
    ModelOutputValidation,
    InternalServer,
    NotFound,
    GuardrailViolation,
    HookExecution,
    Cache,
    McpTool,
}

impl ErrorType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EndpointDefinition => "ENDPOINT_DEFINITION_ERROR",
            Self::InputValidation => "INPUT_VALIDATION_ERROR",
            Self::Authentication => "AUTHENTICATION_ERROR",
            Self::PromptRendering => "PROMPT_RENDERING_ERROR",
            Self::ModelProvider => "MODEL_PROVIDER_ERROR",
            Self::ModelJsonParse => "MODEL_JSON_PARSE_ERROR",
            Self::ModelOutputValidation => "MODEL_OUTPUT_VALIDATION_ERROR",
            Self::InternalServer => "INTERNAL_SERVER_ERROR",
            Self::NotFound => "NOT_FOUND",
            Self::GuardrailViolation => "GUARDRAIL_VIOLATION",
            Self::HookExecution => "HOOK_EXECUTION_ERROR",
            Self::Cache => "CACHE_ERROR",
            Self::McpTool => "MCP_TOOL_ERROR",
        }
    }

    pub fn status_code(self) -> StatusCode {
        match self {
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::InputValidation => StatusCode::BAD_REQUEST,
            Self::Authentication => StatusCode::UNAUTHORIZED,
            Self::ModelProvider | Self::ModelJsonParse | Self::ModelOutputValidation => {
                StatusCode::BAD_GATEWAY
            }
            Self::GuardrailViolation | Self::HookExecution => StatusCode::FORBIDDEN,
            Self::Cache => StatusCode::INTERNAL_SERVER_ERROR,
            Self::McpTool => StatusCode::BAD_GATEWAY,
            Self::EndpointDefinition | Self::PromptRendering | Self::InternalServer => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum AiRestError {
    #[error("{message}")]
    Typed {
        error_type: ErrorType,
        message: String,
        details: Option<Value>,
    },
}

impl AiRestError {
    pub fn new(error_type: ErrorType, message: impl Into<String>) -> Self {
        Self::Typed {
            error_type,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        error_type: ErrorType,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        Self::Typed {
            error_type,
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn error_type(&self) -> ErrorType {
        match self {
            Self::Typed { error_type, .. } => *error_type,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Typed { message, .. } => message,
        }
    }

    pub fn details(&self) -> Option<&Value> {
        match self {
            Self::Typed { details, .. } => details.as_ref(),
        }
    }
}

pub struct ErrorResponse {
    pub request_id: String,
    pub endpoint: Option<String>,
    pub version: Option<String>,
    pub error: AiRestError,
    pub(crate) type_override: Option<String>,
    pub(crate) status_override: Option<StatusCode>,
}

impl ErrorResponse {
    pub fn new(request_id: String, error: AiRestError) -> Self {
        Self {
            request_id,
            endpoint: None,
            version: None,
            error,
            type_override: None,
            status_override: None,
        }
    }

    pub fn for_endpoint(
        request_id: String,
        endpoint: &EndpointDefinition,
        error: AiRestError,
    ) -> Self {
        let mut response = Self {
            request_id,
            endpoint: Some(endpoint.name.clone()),
            version: Some(endpoint.version.clone()),
            error,
            type_override: None,
            status_override: None,
        };
        response.apply_endpoint_overrides(endpoint);
        response
    }

    fn apply_endpoint_overrides(&mut self, endpoint: &EndpointDefinition) {
        let Some(errors) = &endpoint.errors else {
            return;
        };
        let Some(override_cfg) = errors.for_type(self.error.error_type()) else {
            return;
        };

        self.error = match self.error.details().cloned() {
            Some(details) => AiRestError::with_details(
                self.error.error_type(),
                override_cfg.message.clone(),
                details,
            ),
            None => AiRestError::new(self.error.error_type(), override_cfg.message.clone()),
        };
        self.type_override = override_cfg.r#type.clone();
        self.status_override = override_cfg.status.and_then(|code| StatusCode::from_u16(code).ok());
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = self
            .status_override
            .unwrap_or_else(|| self.error.error_type().status_code());
        let error_type = self
            .type_override
            .as_deref()
            .unwrap_or_else(|| self.error.error_type().as_str());

        let mut error_obj = json!({
            "type": error_type,
            "message": self.error.message(),
        });

        if let Some(details) = self.error.details() {
            error_obj["details"] = details.clone();
        }

        let mut meta = json!({ "requestId": self.request_id });
        if let Some(endpoint) = self.endpoint {
            meta["endpoint"] = json!(endpoint);
        }
        if let Some(version) = self.version {
            meta["version"] = json!(version);
        }

        let body = json!({
            "success": false,
            "error": error_obj,
            "meta": meta,
        });

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::{minimal_test_endpoint, EndpointErrors, ErrorOverride};

    fn sample_endpoint(errors: Option<EndpointErrors>) -> EndpointDefinition {
        let mut endpoint = minimal_test_endpoint();
        endpoint.name = "demo".to_string();
        endpoint.path = "/v1/demo".to_string();
        endpoint.errors = errors;
        endpoint
    }

    #[test]
    fn applies_custom_error_message_from_endpoint_definition() {
        let endpoint = sample_endpoint(Some(EndpointErrors {
            input_validation: Some(ErrorOverride {
                message: "Custom input failure".to_string(),
                r#type: None,
                status: None,
            }),
            ..Default::default()
        }));

        let response = ErrorResponse::for_endpoint(
            "req_test".to_string(),
            &endpoint,
            AiRestError::new(
                ErrorType::InputValidation,
                "Request body does not match input schema.",
            ),
        );

        assert_eq!(response.error.message(), "Custom input failure");
    }

    #[test]
    fn applies_custom_error_type_and_status_from_endpoint_definition() {
        let endpoint = sample_endpoint(Some(EndpointErrors {
            authentication: Some(ErrorOverride {
                message: "Bad key".to_string(),
                r#type: Some("CUSTOM_AUTH_ERROR".to_string()),
                status: Some(403),
            }),
            ..Default::default()
        }));

        let response = ErrorResponse::for_endpoint(
            "req_test".to_string(),
            &endpoint,
            AiRestError::new(ErrorType::Authentication, "Missing or invalid API key."),
        );

        assert_eq!(response.error.message(), "Bad key");
        assert_eq!(response.status_override, Some(StatusCode::FORBIDDEN));
        assert_eq!(response.type_override.as_deref(), Some("CUSTOM_AUTH_ERROR"));
    }
}
