use airest::definitions::{
    minimal_test_endpoint, validate_endpoint_definition, GuardrailHook, GuardrailSpec,
};
use airest::errors::ErrorType;
use airest::guardrails::{run_hook, GuardrailChain, GuardrailContext, GuardrailEngine};
use serde_json::json;

#[tokio::test]
async fn pre_cache_write_guardrail_blocks_cache_store() {
    let specs = vec![GuardrailSpec::builtin(
        "regex-block",
        GuardrailHook::PreCacheWrite,
        json!({
            "target": "output",
            "patterns": ["DO_NOT_CACHE"]
        }),
    )];

    let mut endpoint = minimal_test_endpoint();
    endpoint.guardrails = Some(specs.clone());
    let chain = GuardrailChain::compile(&specs, None, &GuardrailEngine::new()).unwrap();

    let ctx = GuardrailContext {
        request_id: "req_cache",
        endpoint: &endpoint,
        hook: GuardrailHook::PreCacheWrite,
        input: json!({ "text": "hello" }),
        rendered_system: None,
        rendered_user: None,
        llm_raw: None,
        output: Some(json!({ "answer": "DO_NOT_CACHE this" })),
        request_body_bytes: 10,
    };

    let err = run_hook(Some(&chain), &endpoint, GuardrailHook::PreCacheWrite, ctx, None, None)
        .await
        .unwrap_err();
    assert_eq!(err.error_type(), ErrorType::GuardrailViolation);
}

#[test]
fn topic_allowlist_builtin_validates() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.guardrails = Some(vec![GuardrailSpec::builtin(
        "topic-allowlist",
        GuardrailHook::PreInput,
        json!({ "allowedTopics": ["finance"] }),
    )]);
    assert!(validate_endpoint_definition(&endpoint).is_ok());
}

#[test]
fn output_secret_scan_builtin_validates() {
    let mut endpoint = minimal_test_endpoint();
    endpoint.guardrails = Some(vec![GuardrailSpec::builtin(
        "output-secret-scan",
        GuardrailHook::PostOutput,
        json!({}),
    )]);
    assert!(validate_endpoint_definition(&endpoint).is_ok());
}
