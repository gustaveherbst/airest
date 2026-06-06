use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use crate::config::Config;
use crate::definitions::EndpointDefinition;
use crate::guardrails::metrics::GuardrailMetrics;
use crate::llm::TokenUsage;

/// Runtime telemetry: structured tracing events + optional OTEL metric export.
#[derive(Clone, Default)]
pub struct TelemetryState {
    pub export_enabled: bool,
    requests_in_flight: Arc<AtomicI64>,
    #[cfg(feature = "otel")]
    pub otel: Option<Arc<OtelMetrics>>,
}

#[cfg(feature = "otel")]
pub struct OtelMetrics {
    requests_total: opentelemetry::metrics::Counter<u64>,
    request_duration_ms: opentelemetry::metrics::Histogram<f64>,
    guardrail_evaluations: opentelemetry::metrics::Counter<u64>,
    llm_duration_ms: opentelemetry::metrics::Histogram<f64>,
    cache_hits: opentelemetry::metrics::Counter<u64>,
    cache_misses: opentelemetry::metrics::Counter<u64>,
    cache_similarity: opentelemetry::metrics::Histogram<f64>,
    cache_tokens_saved: opentelemetry::metrics::Counter<u64>,
    gen_ai_input_tokens: opentelemetry::metrics::Counter<u64>,
    gen_ai_output_tokens: opentelemetry::metrics::Counter<u64>,
    guardrail_blocks: opentelemetry::metrics::Counter<u64>,
    llm_retries: opentelemetry::metrics::Counter<u64>,
    validation_failures: opentelemetry::metrics::Counter<u64>,
    requests_in_flight: opentelemetry::metrics::UpDownCounter<i64>,
}

#[cfg(feature = "otel")]
impl OtelMetrics {
    pub fn new() -> Option<Arc<Self>> {
        use opentelemetry::global;
        use opentelemetry::metrics::{Counter, Histogram, Meter};

        let meter = global::meter("airest");
        Some(Arc::new(Self {
            requests_total: meter
                .u64_counter("airest.requests.total")
                .with_description("Total aiREST requests processed")
                .build(),
            request_duration_ms: meter
                .f64_histogram("airest.request.duration_ms")
                .with_description("End-to-end request latency in milliseconds")
                .build(),
            guardrail_evaluations: meter
                .u64_counter("airest.guardrail.evaluations")
                .with_description("Guardrail module evaluations")
                .build(),
            llm_duration_ms: meter
                .f64_histogram("airest.llm.duration_ms")
                .with_description("LLM call latency in milliseconds")
                .build(),
            cache_hits: meter
                .u64_counter("airest.cache.hit")
                .with_description("Cache hits")
                .build(),
            cache_misses: meter
                .u64_counter("airest.cache.miss")
                .with_description("Cache misses")
                .build(),
            cache_similarity: meter
                .f64_histogram("airest.cache.similarity")
                .with_description("Semantic cache similarity score on hits")
                .build(),
            cache_tokens_saved: meter
                .u64_counter("airest.cache.tokens_saved")
                .with_description("Estimated LLM output tokens avoided by cache hits")
                .build(),
            gen_ai_input_tokens: meter
                .u64_counter("gen_ai.usage.input_tokens")
                .with_description("LLM input tokens consumed")
                .build(),
            gen_ai_output_tokens: meter
                .u64_counter("gen_ai.usage.output_tokens")
                .with_description("LLM output tokens consumed")
                .build(),
            guardrail_blocks: meter
                .u64_counter("airest.guardrail.blocks")
                .with_description("Guardrail blocks by module")
                .build(),
            llm_retries: meter
                .u64_counter("airest.llm.retries")
                .with_description("LLM retries due to parse or schema validation failures")
                .build(),
            validation_failures: meter
                .u64_counter("airest.validation.failures")
                .with_description("Input, output, and JSON parse validation failures")
                .build(),
            requests_in_flight: meter
                .i64_up_down_counter("airest.requests.in_flight")
                .with_description("Requests currently being processed")
                .build(),
        }))
    }

    fn record_counter(
        counter: &opentelemetry::metrics::Counter<u64>,
        endpoint: &str,
        attrs: Vec<(String, String)>,
    ) {
        Self::record_counter_with_value(counter, endpoint, 1, attrs);
    }

    fn record_counter_with_value(
        counter: &opentelemetry::metrics::Counter<u64>,
        endpoint: &str,
        value: u64,
        attrs: Vec<(String, String)>,
    ) {
        use opentelemetry::KeyValue;
        let mut kvs = vec![KeyValue::new("airest.endpoint", endpoint.to_string())];
        for (k, v) in attrs {
            kvs.push(KeyValue::new(k, v));
        }
        counter.add(value, &kvs);
    }

    fn record_histogram(
        histogram: &opentelemetry::metrics::Histogram<f64>,
        endpoint: &str,
        value: f64,
        attrs: Vec<(String, String)>,
    ) {
        use opentelemetry::KeyValue;
        let mut kvs = vec![KeyValue::new("airest.endpoint", endpoint.to_string())];
        for (k, v) in attrs {
            kvs.push(KeyValue::new(k, v));
        }
        histogram.record(value, &kvs);
    }

    fn record_in_flight(
        counter: &opentelemetry::metrics::UpDownCounter<i64>,
        endpoint: &str,
        delta: i64,
    ) {
        use opentelemetry::KeyValue;
        counter.add(delta, &[KeyValue::new("airest.endpoint", endpoint.to_string())]);
    }
}

impl TelemetryState {
    pub fn from_config(config: &Config) -> Self {
        #[cfg(feature = "otel")]
        let otel = if config.otel_enabled && config.otel_metrics {
            OtelMetrics::new()
        } else {
            None
        };

        Self {
            export_enabled: config.otel_enabled,
            requests_in_flight: Arc::new(AtomicI64::new(0)),
            #[cfg(feature = "otel")]
            otel,
        }
    }

    pub fn request_started(&self, endpoint: &str) {
        let current = self.requests_in_flight.fetch_add(1, Ordering::Relaxed) + 1;
        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            OtelMetrics::record_in_flight(&otel.requests_in_flight, endpoint, 1);
        }
        tracing::debug!(
            target: "airest.metrics",
            metric_name = "requests.in_flight",
            endpoint = %endpoint,
            value = current,
        );
    }

    pub fn request_finished(&self, endpoint: &str) {
        let current = self.requests_in_flight.fetch_sub(1, Ordering::Relaxed) - 1;
        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            OtelMetrics::record_in_flight(&otel.requests_in_flight, endpoint, -1);
        }
        tracing::debug!(
            target: "airest.metrics",
            metric_name = "requests.in_flight",
            endpoint = %endpoint,
            value = current,
        );
    }

    pub fn record_llm_retry(&self, endpoint: &str, reason: &str) {
        tracing::info!(
            target: "airest.metrics",
            metric_name = "llm.retry",
            endpoint = %endpoint,
            retry_reason = %reason,
        );
        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            OtelMetrics::record_counter(
                &otel.llm_retries,
                endpoint,
                vec![("airest.retry.reason".to_string(), reason.to_string())],
            );
        }
    }

    pub fn record_validation_failure(&self, endpoint: &str, stage: &str) {
        tracing::info!(
            target: "airest.metrics",
            metric_name = "validation.failure",
            endpoint = %endpoint,
            validation_stage = %stage,
        );
        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            OtelMetrics::record_counter(
                &otel.validation_failures,
                endpoint,
                vec![("airest.validation.stage".to_string(), stage.to_string())],
            );
        }
    }

    pub fn endpoint_enabled(&self, endpoint: &EndpointDefinition) -> bool {
        if !self.export_enabled {
            return false;
        }
        endpoint
            .telemetry
            .as_ref()
            .map(|t| t.enabled)
            .unwrap_or(true)
    }

    pub fn record_request(&self, endpoint: &str, status: &str, duration_ms: u64) {
        tracing::info!(
            target: "airest.metrics",
            metric_name = "request.completed",
            endpoint = %endpoint,
            status = %status,
            latency_ms = duration_ms,
        );

        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            OtelMetrics::record_counter(
                &otel.requests_total,
                endpoint,
                vec![("airest.status".to_string(), status.to_string())],
            );
            OtelMetrics::record_histogram(
                &otel.request_duration_ms,
                endpoint,
                duration_ms as f64,
                vec![("airest.status".to_string(), status.to_string())],
            );
        }
    }

    pub fn record_guardrail_metrics(&self, endpoint: &str, metrics: &GuardrailMetrics) {
        for entry in &metrics.modules {
            tracing::info!(
                target: "airest.metrics",
                metric_name = "guardrail.evaluated",
                endpoint = %endpoint,
                guardrail_module = %entry.module,
                guardrail_runtime = %entry.runtime,
                guardrail_hook = %entry.hook,
                guardrail_outcome = %entry.outcome,
            );

            #[cfg(feature = "otel")]
            if let Some(otel) = &self.otel {
                OtelMetrics::record_counter(
                    &otel.guardrail_evaluations,
                    endpoint,
                    vec![
                        ("airest.guardrail.module".to_string(), entry.module.clone()),
                        ("airest.guardrail.runtime".to_string(), entry.runtime.clone()),
                        ("airest.guardrail.hook".to_string(), entry.hook.clone()),
                        ("airest.guardrail.outcome".to_string(), entry.outcome.clone()),
                    ],
                );
                if entry.outcome == "block" {
                    OtelMetrics::record_counter(
                        &otel.guardrail_blocks,
                        endpoint,
                        vec![("airest.guardrail.module".to_string(), entry.module.clone())],
                    );
                }
            }
        }
    }

    pub fn record_llm(&self, endpoint: &str, provider: &str, model: &str, duration_ms: u64) {
        tracing::info!(
            target: "airest.metrics",
            metric_name = "llm.completed",
            endpoint = %endpoint,
            gen_ai_system = %provider,
            gen_ai_model = %model,
            latency_ms = duration_ms,
        );

        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            OtelMetrics::record_histogram(
                &otel.llm_duration_ms,
                endpoint,
                duration_ms as f64,
                vec![
                    ("gen_ai.system".to_string(), provider.to_string()),
                    ("gen_ai.request.model".to_string(), model.to_string()),
                ],
            );
        }
    }

    pub fn record_mcp_tool(&self, endpoint: &str, tool: &str, success: bool) {
        tracing::info!(
            target: "airest.metrics",
            metric_name = "mcp.tool.called",
            endpoint = %endpoint,
            mcp_tool = %tool,
            success = success,
        );
    }

    pub fn record_token_usage(
        &self,
        endpoint: &str,
        provider: &str,
        model: &str,
        usage: &TokenUsage,
    ) {
        tracing::info!(
            target: "airest.metrics",
            metric_name = "llm.tokens",
            endpoint = %endpoint,
            gen_ai_system = %provider,
            gen_ai_model = %model,
            input_tokens = ?usage.input_tokens,
            output_tokens = ?usage.output_tokens,
        );

        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            let attrs = vec![
                ("gen_ai.system".to_string(), provider.to_string()),
                ("gen_ai.request.model".to_string(), model.to_string()),
            ];
            if let Some(input) = usage.input_tokens {
                OtelMetrics::record_counter_with_value(
                    &otel.gen_ai_input_tokens,
                    endpoint,
                    input as u64,
                    attrs.clone(),
                );
            }
            if let Some(output) = usage.output_tokens {
                OtelMetrics::record_counter_with_value(
                    &otel.gen_ai_output_tokens,
                    endpoint,
                    output as u64,
                    attrs,
                );
            }
        }
    }

    pub fn record_cache(
        &self,
        endpoint: &str,
        hit: bool,
        similarity: Option<f64>,
        estimated_tokens_saved: Option<u64>,
    ) {
        tracing::info!(
            target: "airest.metrics",
            metric_name = "cache.lookup",
            endpoint = %endpoint,
            cache_hit = hit,
            cache_similarity = ?similarity,
            estimated_tokens_saved = ?estimated_tokens_saved,
        );

        #[cfg(feature = "otel")]
        if let Some(otel) = &self.otel {
            if hit {
                OtelMetrics::record_counter(&otel.cache_hits, endpoint, vec![]);
                if let Some(score) = similarity {
                    OtelMetrics::record_histogram(
                        &otel.cache_similarity,
                        endpoint,
                        score,
                        vec![],
                    );
                }
            } else {
                OtelMetrics::record_counter(&otel.cache_misses, endpoint, vec![]);
            }
            if let Some(tokens) = estimated_tokens_saved {
                OtelMetrics::record_counter_with_value(
                    &otel.cache_tokens_saved,
                    endpoint,
                    tokens,
                    vec![],
                );
            }
        }
    }
}
