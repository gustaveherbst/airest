use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;

use airest::auth::JwksCache;
use airest::cache::CacheStore;
use tokio::sync::Semaphore;
use airest::definitions::{load_endpoint_definitions, validate_endpoint_definition};
use airest::prompts::render_prompt;
use airest::runtime::parse_model_json;
use airest::validation::{validate_input, validate_output};
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn test_app_state(
    config: airest::config::Config,
    providers: airest::llm::ProviderConfig,
    registry: airest::definitions::EndpointRegistry,
) -> airest::server::AppState {
    airest::server::AppState {
        cache: Arc::new(CacheStore::new(&config)),
        config: Arc::new(config),
        llm: Arc::new(airest::llm::LlmRouter::new(providers.clone())),
        registry,
        jwks: Arc::new(JwksCache::default()),
        jti_denylist: Arc::new(airest::auth::JtiDenylist::default()),
        http: Arc::new(reqwest::Client::new()),
        guardrails: airest::guardrails::GuardrailEngine::new(),
        telemetry: airest::otel::TelemetryState::default(),
        concurrency: Arc::new(Semaphore::new(64)),
        accepting_requests: Arc::new(AtomicBool::new(true)),
        in_flight: Arc::new(AtomicUsize::new(0)),
    }
}

#[test]
fn loads_yaml_endpoint_definitions_from_directory() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("sentiment.yaml");
    fs::write(
        &file,
        r#"
name: sentiment-analyzer
version: 1.0.0
method: POST
path: /v1/analyze-sentiment
inputSchema:
  type: object
  properties:
    text:
      type: string
systemPrompt: Analyze sentiment.
outputSchema:
  type: object
  properties:
    sentiment:
      type: string
model:
  provider: openai
  model: gpt-4.1-mini
"#,
    )
    .unwrap();

    let defs = load_endpoint_definitions(dir.path()).unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "sentiment-analyzer");
}

#[test]
fn accepts_get_and_post_methods_in_definitions() {
    use airest::definitions::validate_endpoint_definition;

    for method in ["GET", "POST", "get", "post"] {
        let mut def = airest::definitions::minimal_test_endpoint();
        def.name = "sample".to_string();
        def.method = method.to_string();
        def.path = "/v1/sample".to_string();
        def.input_schema =
            json!({ "type": "object", "properties": { "text": { "type": "string" } } });
        def.output_schema =
            json!({ "type": "object", "properties": { "result": { "type": "string" } } });

        assert!(
            validate_endpoint_definition(&def).is_ok(),
            "expected {method} to be accepted"
        );
    }
}

#[tokio::test]
async fn get_endpoint_accepts_query_parameters() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::path::PathBuf;
    use tower::ServiceExt;

    use airest::config::Config;
    use airest::definitions::{load_endpoint_definitions, EndpointRegistry};
    use airest::llm::ProviderConfig;
    use airest::server::build_router_from_state;

    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("quick-sentiment.yaml"),
        r#"
name: quick-sentiment
version: 1.0.0
method: GET
path: /v1/quick-sentiment
inputSchema:
  type: object
  required: [text]
  properties:
    text:
      type: string
      minLength: 1
systemPrompt: Analyze sentiment and return JSON only.
outputSchema:
  type: object
  required: [sentiment]
  properties:
    sentiment:
      type: string
model:
  provider: openai
  model: gpt-4.1-mini
"#,
    )
    .unwrap();

    let mut providers = ProviderConfig::from_env();
    providers.openai.api_key = "test-key".to_string();
    providers.openai.base_url = "http://localhost".to_string();

    let registry = EndpointRegistry::from_definitions(
        load_endpoint_definitions(dir.path()).unwrap(),
    );
    let mut config = Config::for_test(None, PathBuf::from("."));
    config.providers = providers.clone();
    let state = test_app_state(config, providers, registry);

    let app = build_router_from_state(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/quick-sentiment?text=hello")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

#[test]
fn rejects_invalid_endpoint_definition() {
    let mut def = airest::definitions::minimal_test_endpoint();
    def.name = "".to_string();
    def.method = "PUT".to_string();
    def.path = "invalid".to_string();
    def.model.provider = "anthropic".to_string();
    def.model.model = "claude".to_string();

    assert!(validate_endpoint_definition(&def).is_err());
}

#[test]
fn accepts_supported_model_providers() {
    use airest::llm::ProviderKind;

    for value in [
        "openai",
        "azure_openai",
        "azure",
        "anthropic",
        "gemini",
        "google",
        "grok",
        "xai",
        "ollama",
    ] {
        assert!(ProviderKind::parse(value).is_ok(), "expected {value} to parse");
    }

    assert!(ProviderKind::parse("unknown-vendor").is_err());
}

#[test]
fn validates_input_and_output_schemas() {
    let input_schema = json!({
        "type": "object",
        "required": ["text"],
        "properties": { "text": { "type": "string", "minLength": 1 } }
    });
    let output_schema = json!({
        "type": "object",
        "required": ["sentiment", "confidence"],
        "properties": {
            "sentiment": { "type": "string" },
            "confidence": { "type": "number" }
        }
    });

    assert!(validate_input(&input_schema, &json!({})).is_err());
    assert!(validate_input(&input_schema, &json!({ "text": "hi" })).is_ok());

    let valid_output = json!({ "sentiment": "positive", "confidence": 0.9 });
    assert!(validate_output(&output_schema, &valid_output).is_ok());

    let invalid_output = json!({ "sentiment": "positive" });
    assert!(validate_output(&output_schema, &invalid_output).is_err());
}

#[test]
fn renders_prompt_templates() {
    let rendered = render_prompt(
        "system prompt",
        Some("Analyze: {{text}}"),
        &json!({ "type": "object", "properties": { "sentiment": { "type": "string" } } }),
        &json!({ "text": "Great product!" }),
    )
    .unwrap();

    assert_eq!(rendered.system, "system prompt");
    assert!(rendered.user.contains("Great product!"));
    assert!(rendered.user.contains("You must return only valid JSON"));
}

#[test]
fn parses_json_from_model_output() {
    let parsed = parse_model_json(r#"{"sentiment":"positive"}"#, true).unwrap();
    assert_eq!(parsed["sentiment"], "positive");

    let fenced = parse_model_json("```json\n{\"sentiment\":\"neutral\"}\n```", true).unwrap();
    assert_eq!(fenced["sentiment"], "neutral");
}

#[test]
fn ignores_json_definition_files() {
    let dir = TempDir::new().unwrap();
    fs::write(
        &dir.path().join("ignored.json"),
        r#"{"name":"json-api","version":"1.0.0","method":"POST","path":"/v1/json"}"#,
    )
    .unwrap();
    fs::write(
        &dir.path().join("valid.yaml"),
        r#"
name: yaml-api
version: 1.0.0
method: POST
path: /v1/yaml
inputSchema:
  type: object
  properties:
    text:
      type: string
systemPrompt: Test.
outputSchema:
  type: object
  properties:
    result:
      type: string
model:
  provider: openai
  model: gpt-4.1-mini
"#,
    )
    .unwrap();

    let defs = load_endpoint_definitions(dir.path()).unwrap();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "yaml-api");
}

#[tokio::test]
async fn returns_structured_not_found_response() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::path::PathBuf;
    use tower::ServiceExt;

    use airest::config::Config;
    use airest::definitions::{load_endpoint_definitions_with_options, EndpointRegistry, LoadOptions};
    use airest::guardrails::GuardrailEngine;
    use airest::llm::ProviderConfig;
    use airest::server::build_router_from_state;

    let mut providers = ProviderConfig::from_env();
    providers.openai.api_key = "test-key".to_string();
    providers.openai.base_url = "http://localhost".to_string();

    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples"),
        LoadOptions { recursive: true },
    )
    .unwrap();
    let registry = EndpointRegistry::from_loaded(&loaded, &GuardrailEngine::new());
    let mut config = Config::for_test(Some("test-key".to_string()), PathBuf::from("./examples"));
    config.providers = providers.clone();
    let state = test_app_state(config, providers, registry);

    let app = build_router_from_state(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/unknown-endpoint")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["type"], "NOT_FOUND");
    assert_eq!(json["error"]["message"], "No matching aiREST endpoint.");
    assert!(json["meta"]["requestId"].as_str().unwrap().starts_with("req_"));
}

#[tokio::test]
async fn endpoint_health_returns_default_or_custom_response() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::path::PathBuf;
    use tower::ServiceExt;

    use airest::config::Config;
    use airest::definitions::{load_endpoint_definitions_with_options, EndpointRegistry, LoadOptions};
    use airest::guardrails::GuardrailEngine;
    use airest::llm::ProviderConfig;
    use airest::server::build_router_from_state;

    let mut providers = ProviderConfig::from_env();
    providers.openai.api_key = "test-key".to_string();
    providers.openai.base_url = "http://localhost".to_string();

    let loaded = load_endpoint_definitions_with_options(
        std::path::Path::new("./examples"),
        LoadOptions { recursive: true },
    )
    .unwrap();
    let registry = EndpointRegistry::from_loaded(&loaded, &GuardrailEngine::new());
    let mut config = Config::for_test(Some("test-key".to_string()), PathBuf::from("./examples"));
    config.providers = providers.clone();
    let state = test_app_state(config, providers, registry);

    let app = build_router_from_state(state);

    let custom = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/analyze-contract-risk/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(custom.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&axum::body::to_bytes(custom.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    assert_eq!(body["message"], "Contract risk analyzer is loaded and ready.");

    let default = app
        .oneshot(
            Request::builder()
                .uri("/v1/analyze-sentiment/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(default.status(), StatusCode::OK);
}

#[tokio::test]
async fn support_folder_loads_three_endpoints() {
    let defs = load_endpoint_definitions(std::path::Path::new("./examples/support")).unwrap();
    assert_eq!(defs.len(), 3);

    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"support-ticket-triage"));
    assert!(names.contains(&"support-reply-suggester"));
    assert!(names.contains(&"support-escalation-advisor"));
}

#[test]
fn loads_bundled_example_definitions() {
    let defs = load_endpoint_definitions(std::path::Path::new("./examples")).unwrap();
    assert_eq!(defs.len(), 18);

    let sentiment = defs
        .iter()
        .find(|d| d.name == "sentiment-analyzer")
        .expect("sentiment-analyzer should be loaded from analytics/sentiment-analyzer.yaml");
    assert_eq!(sentiment.path, "/v1/analyze-sentiment");
}

#[test]
fn generates_openapi_document() {
    use airest::definitions::load_endpoint_definitions;
    use airest::openapi::generate_openapi;

    let defs = load_endpoint_definitions(std::path::Path::new("./examples")).unwrap();
    let spec = generate_openapi(&defs, "http://localhost:3300");
    assert_eq!(spec["openapi"], "3.0.3");
    assert!(spec["paths"].as_object().unwrap().contains_key("/v1/analyze-sentiment"));
}

#[test]
fn loads_endpoint_with_custom_errors() {
    let defs = load_endpoint_definitions(std::path::Path::new("./examples")).unwrap();
    let contract = defs
        .iter()
        .find(|d| d.name == "contract-risk-analyzer")
        .expect("contract endpoint");
    let errors = contract.errors.as_ref().expect("custom errors");
    assert!(errors.input_validation.is_some());
}
