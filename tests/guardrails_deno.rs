use airest::definitions::{minimal_test_endpoint, GuardrailHook, GuardrailSpec};
use serde_json::json;
use airest::errors::ErrorType;
use airest::guardrails::{
    run_hook, GuardrailChain, GuardrailContext, GuardrailEngine,
};

#[tokio::test]
async fn deno_guardrail_blocks_when_text_too_long() {
    let script = r#"
function evaluate(ctx) {
  const text = ctx.input?.text ?? "";
  if (text.length > 20) {
    return { action: "block", message: "Text exceeds maximum length for this tier" };
  }
  return { action: "pass" };
}
"#;

    let specs = vec![GuardrailSpec::deno_script(
        "text-length-tier",
        GuardrailHook::PostInput,
        script,
    )];

    let mut endpoint = minimal_test_endpoint();
    endpoint.guardrails = Some(specs.clone());
    let chain = GuardrailChain::compile(&specs, None, &GuardrailEngine::new()).unwrap();

    let ctx = GuardrailContext {
        request_id: "req_deno_block",
        endpoint: &endpoint,
        hook: GuardrailHook::PostInput,
        input: json!({ "text": "this message is definitely longer than twenty characters" }),
        rendered_system: None,
        rendered_user: None,
        llm_raw: None,
        output: None,
        request_body_bytes: 0,
    };

    let err = run_hook(Some(&chain), &endpoint, GuardrailHook::PostInput, ctx, None, None)
        .await
        .unwrap_err();
    assert_eq!(err.error_type(), ErrorType::GuardrailViolation);
    assert!(err.message().contains("maximum length"));
}

#[tokio::test]
async fn deno_guardrail_modifies_input_email() {
    let script = r#"
function evaluate(ctx) {
  if (ctx.input && ctx.input.email) {
    const input = Object.assign({}, ctx.input);
    input.email = "[REDACTED]";
    return { action: "modify", input: input };
  }
  return { action: "pass" };
}
"#;

    let specs = vec![GuardrailSpec::deno_script("email-mask", GuardrailHook::PostInput, script)];

    let mut endpoint = minimal_test_endpoint();
    endpoint.guardrails = Some(specs.clone());
    let chain = GuardrailChain::compile(&specs, None, &GuardrailEngine::new()).unwrap();

    let ctx = GuardrailContext {
        request_id: "req_deno_modify",
        endpoint: &endpoint,
        hook: GuardrailHook::PostInput,
        input: json!({ "email": "user@example.com", "text": "hello" }),
        rendered_system: None,
        rendered_user: None,
        llm_raw: None,
        output: None,
        request_body_bytes: 0,
    };

    let result = run_hook(Some(&chain), &endpoint, GuardrailHook::PostInput, ctx, None, None)
        .await
        .unwrap();
    assert_eq!(result.input["email"], "[REDACTED]");
    assert_eq!(result.input["text"], "hello");
}

#[tokio::test]
async fn deno_guardrail_rejects_forbidden_sandbox_api() {
    let script = "function evaluate(ctx) { fetch('http://evil'); return { action: 'pass' }; }";
    let specs = vec![GuardrailSpec::deno_script("evil-fetch", GuardrailHook::PreInput, script)];

    match GuardrailChain::compile(&specs, None, &GuardrailEngine::new()) {
        Err(err) => {
            assert_eq!(err.error_type(), ErrorType::GuardrailViolation);
            assert!(err.message().contains("forbidden"));
        }
        Ok(_) => panic!("expected forbidden sandbox pattern to be rejected"),
    }
}
