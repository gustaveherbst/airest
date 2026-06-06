use airest::cache::{exact_cache_key, CacheStore};
use airest::config::Config;
use airest::definitions::{
    minimal_test_endpoint, validate_endpoint_definition, CacheConfig, GuardrailHook,
    GuardrailSpec,
};
use airest::errors::ErrorType;
use airest::guardrails::{run_hook, GuardrailChain, GuardrailContext, GuardrailEngine};
use serde_json::json;

#[test]
fn validates_guardrail_configuration() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.guardrails = Some(vec![GuardrailSpec::builtin(
        "max-request-size",
        GuardrailHook::PreInput,
        json!({ "maxBytes": 1024 }),
    )]);
    assert!(validate_endpoint_definition(&endpoint).is_ok());
}

#[tokio::test]
async fn max_request_size_guardrail_blocks_large_body() {
    let specs = vec![GuardrailSpec::builtin(
        "max-request-size",
        GuardrailHook::PreInput,
        json!({ "maxBytes": 16 }),
    )];

    let mut endpoint = minimal_test_endpoint();
    endpoint.guardrails = Some(specs.clone());
    let chain = GuardrailChain::compile(&specs, None, &GuardrailEngine::new()).unwrap();

    let ctx = GuardrailContext {
        request_id: "req_test",
        endpoint: &endpoint,
        hook: GuardrailHook::PreInput,
        input: json!({}),
        rendered_system: None,
        rendered_user: None,
        llm_raw: None,
        output: None,
        request_body_bytes: 100,
    };

    let err = run_hook(Some(&chain), &endpoint, GuardrailHook::PreInput, ctx, None, None)
        .await
        .unwrap_err();
    assert_eq!(err.error_type(), ErrorType::GuardrailViolation);
}

#[tokio::test]
async fn exact_cache_returns_hit_on_second_lookup() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.cache = Some(CacheConfig {
        enabled: true,
        mode: Some("exact".to_string()),
        similarity_threshold: None,
        ttl_seconds: Some(3600),
        max_entries: Some(100),
        scope: None,
        exclude_fields: None,
        embedder: None,
        store: None,
        bypass_on_guardrail_block: None,
    });

    let config = Config::for_test(None, std::path::PathBuf::from("api"));
    let store = CacheStore::new(&config);
    let input = json!({ "text": "hello world" });
    let output = json!({ "sentiment": "positive" });
    let config = endpoint.cache.as_ref().unwrap();

    store
        .store(&endpoint, config, &input, &output, None)
        .await
        .unwrap();

    let lookup = store.lookup(&endpoint, config, &input, None).await.unwrap();
    assert!(matches!(lookup, airest::cache::CacheLookup::Hit { .. }));
}

#[test]
fn exact_cache_key_is_stable() {
    let input = json!({ "text": "hello", "requestId": "skip" });
    let key1 = exact_cache_key("scope", &input, &["requestId".to_string()]);
    let key2 = exact_cache_key("scope", &input, &["requestId".to_string()]);
    assert_eq!(key1, key2);
}

#[test]
fn validates_semantic_cache_threshold() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.cache = Some(CacheConfig {
        enabled: true,
        mode: Some("semantic".to_string()),
        similarity_threshold: Some(1.5),
        ttl_seconds: None,
        max_entries: None,
        scope: None,
        exclude_fields: None,
        embedder: None,
        store: None,
        bypass_on_guardrail_block: None,
    });
    assert!(validate_endpoint_definition(&endpoint).is_err());
}
