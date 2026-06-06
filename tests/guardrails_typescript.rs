use airest::definitions::{minimal_test_endpoint, GuardrailHook, GuardrailSpec};
use airest::guardrails::{run_hook, GuardrailChain, GuardrailContext, GuardrailEngine};
use serde_json::json;
use std::path::Path;

#[tokio::test]
async fn example_typescript_guardrails_execute() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/guardrails");
    let cases = [
        (
            "payment-amount-limit",
            json!({ "amount": 150.0, "currency": "USD" }),
        ),
        (
            "clinical-topic-allowlist",
            json!({ "clinicalNote": "Patient presents with improved mobility after physical therapy." }),
        ),
        (
            "support-priority-cap",
            json!({
                "subject": "Cannot access my account",
                "body": "Password reset failed.",
                "customerTier": "premium"
            }),
        ),
    ];

    for (module, input) in cases {
        let spec = GuardrailSpec {
            module: module.to_string(),
            runtime: Some("deno".to_string()),
            hook: GuardrailHook::PostInput,
            path: Some(format!("{module}.ts")),
            script: None,
            timeout_ms: Some(2000),
            config: match module {
                "payment-amount-limit" => json!({ "maxAmount": 25000 }),
                "clinical-topic-allowlist" => json!({ "blockedTopics": ["controlled substance"] }),
                _ => json!({}),
            },
        };

        let mut endpoint = minimal_test_endpoint();
        endpoint.guardrails = Some(vec![spec.clone()]);
        let chain = GuardrailChain::compile(
            &[spec],
            Some(examples.as_path()),
            &GuardrailEngine::new(),
        )
        .unwrap_or_else(|err| panic!("compile {module}: {err}"));

        let ctx = GuardrailContext {
            request_id: "req_ts_guardrail",
            endpoint: &endpoint,
            hook: GuardrailHook::PostInput,
            input,
            rendered_system: None,
            rendered_user: None,
            llm_raw: None,
            output: None,
            request_body_bytes: 0,
        };

        run_hook(Some(&chain), &endpoint, GuardrailHook::PostInput, ctx, None, None)
            .await
            .unwrap_or_else(|err| panic!("evaluate {module}: {}", err.message()));
    }
}
