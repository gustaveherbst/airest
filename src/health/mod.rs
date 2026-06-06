use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::definitions::{EndpointDefinition, EndpointRegistry};
use crate::llm::{LlmRouter, ProviderConfig};

pub fn global_health_response(registry: &EndpointRegistry) -> Response {
    let endpoints: Vec<_> = registry
        .list()
        .iter()
        .map(endpoint_health_summary)
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "status": "ok",
            "message": "aiREST server is healthy",
            "meta": {
                "endpointCount": endpoints.len(),
                "endpoints": endpoints,
            }
        })),
    )
        .into_response()
}

pub fn readiness_response(
    registry: &EndpointRegistry,
    providers: &ProviderConfig,
    llm: &LlmRouter,
    accepting_requests: bool,
) -> Response {
    let definitions = registry.list();
    let mut checks = Vec::new();
    let mut ready = accepting_requests && !definitions.is_empty();

    checks.push(json!({
        "name": "accepting_requests",
        "ok": accepting_requests,
    }));

    checks.push(json!({
        "name": "endpoints_loaded",
        "ok": !definitions.is_empty(),
        "endpointCount": definitions.len(),
    }));

    let provider_check = provider_readiness(providers, &definitions, llm);
    if !provider_check.ok {
        ready = false;
    }
    checks.push(serde_json::to_value(&provider_check).unwrap_or_else(|_| json!({})));

    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "success": ready,
            "status": if ready { "ready" } else { "not_ready" },
            "checks": checks,
        })),
    )
        .into_response()
}

#[derive(serde::Serialize)]
struct ProviderReadinessCheck {
    name: &'static str,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    open_providers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl ProviderReadinessCheck {
    fn providers_configured() -> Self {
        Self {
            name: "providers_configured",
            ok: true,
            open_providers: None,
            error: None,
        }
    }
}

fn provider_readiness(
    providers: &ProviderConfig,
    definitions: &[EndpointDefinition],
    llm: &LlmRouter,
) -> ProviderReadinessCheck {
    if let Err(err) = crate::definitions::validate_provider_credentials(providers, definitions) {
        return ProviderReadinessCheck {
            name: "providers_configured",
            ok: false,
            open_providers: None,
            error: Some(err.to_string()),
        };
    }

    let mut open = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for endpoint in definitions {
        if let Ok(provider) = endpoint.model.provider_kind() {
            if seen.insert(provider) && llm.circuit_breaker().is_open(provider) {
                open.push(provider.as_str().to_string());
            }
        }
    }

    if open.is_empty() {
        ProviderReadinessCheck::providers_configured()
    } else {
        ProviderReadinessCheck {
            name: "providers_configured",
            ok: false,
            open_providers: Some(open),
            error: Some("One or more LLM provider circuit breakers are open".to_string()),
        }
    }
}

pub fn endpoint_health_response(endpoint: &EndpointDefinition) -> Response {
    let (status, message) = resolve_health(endpoint);

    (
        status,
        Json(json!({
            "success": status.is_success(),
            "status": if status.is_success() { "ok" } else { "degraded" },
            "message": message,
            "meta": {
                "endpoint": endpoint.name,
                "version": endpoint.version,
                "category": endpoint.category,
                "path": endpoint.path,
                "healthPath": endpoint.health_path(),
            }
        })),
    )
        .into_response()
}

fn resolve_health(endpoint: &EndpointDefinition) -> (StatusCode, String) {
    if let Some(health) = &endpoint.health {
        let status = health
            .status
            .and_then(|code| StatusCode::from_u16(code).ok())
            .unwrap_or(StatusCode::OK);
        let message = health
            .message
            .clone()
            .unwrap_or_else(|| format!("{} is healthy", endpoint.name));
        return (status, message);
    }

    (StatusCode::OK, format!("{} is healthy", endpoint.name))
}

fn endpoint_health_summary(endpoint: &EndpointDefinition) -> serde_json::Value {
    let (status, message) = resolve_health(endpoint);
    json!({
        "name": endpoint.name,
        "category": endpoint.category,
        "path": endpoint.path,
        "healthPath": endpoint.health_path(),
        "status": if status.is_success() { "ok" } else { "degraded" },
        "message": message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definitions::minimal_test_endpoint;

    fn sample_endpoint(health: Option<crate::definitions::HealthConfig>) -> EndpointDefinition {
        let mut endpoint = minimal_test_endpoint();
        endpoint.category = Some("demo".to_string());
        endpoint.health = health;
        endpoint
    }

    #[test]
    fn default_endpoint_health_is_200_ok() {
        let endpoint = sample_endpoint(None);
        let (status, message) = resolve_health(&endpoint);
        assert_eq!(status, StatusCode::OK);
        assert!(message.contains("test"));
        assert_eq!(endpoint.health_path(), "/v1/test/health");
    }

    #[test]
    fn custom_endpoint_health_uses_yaml_message_and_status() {
        let endpoint = sample_endpoint(Some(crate::definitions::HealthConfig {
            message: Some("Demo API ready".to_string()),
            status: Some(200),
        }));
        let (status, message) = resolve_health(&endpoint);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(message, "Demo API ready");
    }
}
