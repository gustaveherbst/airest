mod metrics;
mod spans;

pub use metrics::TelemetryState;
pub use spans::{
    apply_auth_attrs, cache_lookup, guardrail_module, hook_execute, llm_complete, llm_retry,
    mcp_tool_call, parse_json, render_prompt, request, validate_input, validate_output,
};

use tracing_subscriber::{fmt, EnvFilter};

use crate::config::Config;

pub fn init_tracing(config: &Config) {
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    #[cfg(feature = "otel")]
    if config.otel_enabled {
        if init_otel_exporter(config) {
            return;
        }
        tracing::warn!("OTEL enabled but OTLP exporter failed to initialize; using fmt logs only");
    }

    let _ = fmt().with_env_filter(filter).try_init();
}

#[cfg(feature = "otel")]
fn init_otel_exporter(config: &Config) -> bool {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
    use opentelemetry_sdk::trace::TracerProvider as SdkTracerProvider;
    use tracing_subscriber::{layer::SubscriberExt, Registry};

    let endpoint = config
        .otel_endpoint
        .clone()
        .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
        .unwrap_or_else(|| "http://localhost:4317".to_string());

    let span_exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint.clone())
        .build()
    {
        Ok(exporter) => exporter,
        Err(err) => {
            tracing::warn!(error = %err, "OTLP span exporter build failed");
            return false;
        }
    };

    let trace_provider = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter, opentelemetry_sdk::runtime::Tokio)
        .with_sampler(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(
            config.otel_sample_ratio,
        ))
        .build();

    opentelemetry::global::set_tracer_provider(trace_provider.clone());

    if config.otel_metrics {
        if let Ok(metric_exporter) = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()
        {
            let reader = PeriodicReader::builder(
                metric_exporter,
                opentelemetry_sdk::runtime::Tokio,
            )
            .build();
            let meter_provider = SdkMeterProvider::builder()
                .with_reader(reader)
                .build();
            opentelemetry::global::set_meter_provider(meter_provider);
        } else {
            tracing::warn!("OTLP metric exporter build failed; metrics will use no-op provider");
        }
    }

    let tracer = trace_provider.tracer("airest");
    let filter = EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    Registry::default()
        .with(filter)
        .with(fmt::layer())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .try_init()
        .is_ok()
}
