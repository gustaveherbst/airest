use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;

use airest::config::production::validate_production_config;
use airest::definitions::{minimal_test_endpoint, EndpointRegistry};
use airest::llm::{LlmRouter, ProviderConfig, ProviderKind};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tokio::sync::Semaphore;
use tower::ServiceExt;

fn test_state(
    config: airest::config::Config,
    providers: ProviderConfig,
    registry: EndpointRegistry,
) -> airest::server::AppState {
    airest::server::AppState {
        cache: Arc::new(airest::cache::CacheStore::new(&config)),
        config: Arc::new(config),
        llm: Arc::new(airest::llm::LlmRouter::new(providers)),
        registry,
        jwks: Arc::new(airest::auth::JwksCache::default()),
        jti_denylist: Arc::new(airest::auth::JtiDenylist::default()),
        http: Arc::new(reqwest::Client::new()),
        guardrails: airest::guardrails::GuardrailEngine::new(),
        telemetry: airest::otel::TelemetryState::default(),
        concurrency: Arc::new(Semaphore::new(8)),
        accepting_requests: Arc::new(AtomicBool::new(true)),
        in_flight: Arc::new(AtomicUsize::new(0)),
    }
}

#[test]
fn production_mode_rejects_placeholder_api_key() {
    let mut config = airest::config::Config::for_test(Some("replace-me".to_string()), ".".into());
    config.production_mode = true;
    let providers = ProviderConfig::from_env();
    let endpoint = minimal_test_endpoint();
    let err = validate_production_config(&config, &providers, &[endpoint]).unwrap_err();
    assert!(err.to_string().contains("placeholder"));
}

#[test]
fn production_mode_rejects_hot_reload() {
    let mut config = airest::config::Config::for_test(Some("real-production-key".to_string()), ".".into());
    config.production_mode = true;
    config.hot_reload = true;
    let mut providers = ProviderConfig::from_env();
    providers.openai.api_key = "real-openai-key".to_string();
    let endpoint = minimal_test_endpoint();
    let err = validate_production_config(&config, &providers, &[endpoint]).unwrap_err();
    assert!(err.to_string().contains("HOT_RELOAD"));
}

#[tokio::test]
async fn ready_endpoint_returns_200_when_configured() {
    let mut config = airest::config::Config::for_test(Some("test-key".to_string()), ".".into());
    config.providers.openai.api_key = "test-key".to_string();
    let providers = config.providers.clone();
    let registry = EndpointRegistry::from_definitions(vec![minimal_test_endpoint()]);
    let state = test_state(config, providers, registry);
    let app = airest::server::build_router_from_state(state);

    let response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn circuit_breaker_opens_after_threshold_failures() {
    let router = LlmRouter::new(ProviderConfig::from_env());
    let breaker = router.circuit_breaker();
    for _ in 0..5 {
        breaker.record_failure(ProviderKind::Openai);
    }
    assert!(breaker.is_open(ProviderKind::Openai));
    breaker.record_success(ProviderKind::Openai);
    assert!(!breaker.is_open(ProviderKind::Openai));
}

#[test]
fn mcp_servers_require_non_empty_tools_allow() {
    use airest::definitions::{minimal_test_endpoint, McpServerConfig, ToolsConfig};

    let mut endpoint = minimal_test_endpoint();
    endpoint.tools = Some(ToolsConfig {
        mcp_servers: Some(vec![McpServerConfig {
            name: "support-kb".to_string(),
            transport: "stdio".to_string(),
            command: Some("node".to_string()),
            args: Some(vec!["./mock.mjs".to_string()]),
            ..Default::default()
        }]),
        allow: None,
        ..Default::default()
    });
    let err = airest::definitions::validate_endpoint_definition(&endpoint).unwrap_err();
    assert!(err.to_string().contains("tools.allow"));
}

#[test]
fn tools_allow_entries_must_be_qualified() {
    use airest::definitions::{minimal_test_endpoint, McpServerConfig, ToolsConfig};

    let mut endpoint = minimal_test_endpoint();
    endpoint.tools = Some(ToolsConfig {
        mcp_servers: Some(vec![McpServerConfig {
            name: "support-kb".to_string(),
            transport: "stdio".to_string(),
            command: Some("node".to_string()),
            args: Some(vec!["./mock.mjs".to_string()]),
            ..Default::default()
        }]),
        allow: Some(vec!["search_tickets".to_string()]),
        ..Default::default()
    });
    let err = airest::definitions::validate_endpoint_definition(&endpoint).unwrap_err();
    assert!(err.to_string().contains("serverName/tool_name"));
}

#[test]
fn hook_permissions_reject_network_wildcard() {
    use airest::definitions::{minimal_test_endpoint, EndpointHooks, HookSpec};

    let mut endpoint = minimal_test_endpoint();
    endpoint.hooks = Some(EndpointHooks {
        post_input: Some(HookSpec {
            runtime: "deno".to_string(),
            timeout_ms: Some(1000),
            permissions: Some(vec!["network:*".to_string()]),
            script: "function transform(input, host) { return input; }".to_string(),
        }),
        ..Default::default()
    });
    let err = airest::definitions::validate_endpoint_definition(&endpoint).unwrap_err();
    assert!(err.to_string().contains("network:*"));
}

#[test]
fn hook_fetch_requires_explicit_network_permissions() {
    use airest::definitions::{minimal_test_endpoint, EndpointHooks, HookSpec};

    let mut endpoint = minimal_test_endpoint();
    endpoint.hooks = Some(EndpointHooks {
        post_input: Some(HookSpec {
            runtime: "deno".to_string(),
            timeout_ms: Some(1000),
            permissions: None,
            script: "function transform(input, host) { host.fetch('https://api.example.com'); return input; }"
                .to_string(),
        }),
        ..Default::default()
    });
    let err = airest::definitions::validate_endpoint_definition(&endpoint).unwrap_err();
    assert!(err.to_string().contains("fetch()"));
}

#[test]
fn pii_example_endpoints_redact_logs() {
    use airest::definitions::{load_endpoint_definitions_with_options, LoadOptions};

    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples"),
        LoadOptions {
            recursive: true,
        },
    )
    .expect("examples load");

    let pii_categories = ["healthcare", "finance", "legal", "support"];
    for item in loaded {
        let category = item.definition.category.as_deref().unwrap_or("");
        if !pii_categories.contains(&category) {
            continue;
        }
        assert!(
            item.definition.policies.redact_inputs,
            "{} must set policies.redactInputs",
            item.definition.name
        );
    }
}

#[test]
fn sensitive_example_endpoints_disable_cache() {
    use airest::definitions::{load_endpoint_definitions_with_options, LoadOptions};

    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples"),
        LoadOptions {
            recursive: true,
        },
    )
    .expect("examples load");

    let sensitive = ["healthcare", "finance", "legal", "support"];
    for item in loaded {
        let category = item.definition.category.as_deref().unwrap_or("");
        if !sensitive.contains(&category) {
            continue;
        }
        let cache_enabled = item
            .definition
            .cache
            .as_ref()
            .is_some_and(|c| c.enabled);
        assert!(
            !cache_enabled,
            "{} must set cache.enabled: false",
            item.definition.name
        );
    }
}

#[test]
fn tool_endpoints_cap_cost_and_timeout() {
    use airest::definitions::{load_endpoint_definitions_with_options, LoadOptions};

    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples"),
        LoadOptions {
            recursive: true,
        },
    )
    .expect("examples load");

    for item in loaded {
        let has_tools = item
            .definition
            .tools
            .as_ref()
            .and_then(|t| t.mcp_servers.as_ref())
            .is_some_and(|s| !s.is_empty());
        if !has_tools {
            continue;
        }
        let policies = item.definition.policies();
        assert!(
            policies.max_tool_rounds <= 5,
            "{} maxToolRounds must be ≤ 5",
            item.definition.name
        );
        assert!(
            policies.tool_timeout_ms <= 10_000,
            "{} toolTimeoutMs must be ≤ 10000",
            item.definition.name
        );
    }
}

#[test]
fn all_example_endpoints_define_guardrails() {
    use airest::definitions::{load_endpoint_definitions_with_options, LoadOptions};

    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples"),
        LoadOptions {
            recursive: true,
        },
    )
    .expect("examples load");

    for item in loaded {
        assert!(
            item
                .definition
                .guardrails
                .as_ref()
                .is_some_and(|g| !g.is_empty()),
            "{} must define production guardrails",
            item.definition.name
        );
    }
}
