use airest::config::Config;
use airest::definitions::minimal_test_endpoint;
use airest::guardrails::metrics::GuardrailMetrics;
use airest::otel::TelemetryState;

#[test]
fn telemetry_state_records_structured_guardrail_metrics_without_otel_feature() {
    let config = Config::for_test(None, std::path::PathBuf::from("."));
    let mut cfg = config;
    cfg.otel_enabled = true;
    let telemetry = TelemetryState::from_config(&cfg);

    let mut metrics = GuardrailMetrics::default();
    metrics.record(
        "max-request-size",
        "builtin",
        "preInput",
        &airest::guardrails::GuardrailOutcome::Pass,
    );

    let endpoint = minimal_test_endpoint();
    assert!(telemetry.endpoint_enabled(&endpoint));
    telemetry.record_guardrail_metrics(&endpoint.name, &metrics);
    telemetry.record_request(&endpoint.name, "ok", 42);
}
