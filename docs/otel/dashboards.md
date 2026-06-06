# OpenTelemetry dashboards

Build with `cargo build --features otel` and set `AIREST_OTEL_ENABLED=true`, `AIREST_OTEL_METRICS=true`, `OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317`. Per-endpoint `telemetry.enabled: true` in YAML exports guardrail evaluation spans and metrics for that route.

## Exported metrics

| Metric | Type | Labels |
|--------|------|--------|
| `airest.requests.total` | counter | `airest.endpoint`, `airest.status` |
| `airest.request.duration_ms` | histogram | `airest.endpoint`, `airest.status` |
| `airest.llm.duration_ms` | histogram | `airest.endpoint`, `gen_ai.system`, `gen_ai.request.model` |
| `gen_ai.usage.input_tokens` | counter | `airest.endpoint`, `gen_ai.system`, `gen_ai.request.model` |
| `gen_ai.usage.output_tokens` | counter | `airest.endpoint`, `gen_ai.system`, `gen_ai.request.model` |
| `airest.cache.hit` | counter | `airest.endpoint` |
| `airest.cache.miss` | counter | `airest.endpoint` |
| `airest.cache.similarity` | histogram | `airest.endpoint` |
| `airest.cache.tokens_saved` | counter | `airest.endpoint` |
| `airest.guardrail.evaluations` | counter | module, runtime, hook, outcome |
| `airest.guardrail.blocks` | counter | `airest.endpoint`, `airest.guardrail.module` |
| `airest.llm.retries` | counter | `airest.endpoint`, `airest.retry.reason` |
| `airest.validation.failures` | counter | `airest.endpoint`, `airest.validation.stage` |
| `airest.requests.in_flight` | up/down counter | `airest.endpoint` |

## Grafana (Prometheus / Mimir)

Example panel queries:

```promql
# Request rate by endpoint
sum(rate(airest_requests_total[5m])) by (airest_endpoint)

# P95 latency
histogram_quantile(0.95, sum(rate(airest_request_duration_ms_bucket[5m])) by (le, airest_endpoint))

# Cache hit ratio
sum(rate(airest_cache_hit[5m])) by (airest_endpoint)
/
(sum(rate(airest_cache_hit[5m])) by (airest_endpoint) + sum(rate(airest_cache_miss[5m])) by (airest_endpoint))

# Estimated tokens saved per minute
sum(rate(airest_cache_tokens_saved[5m])) by (airest_endpoint)

# Guardrail block rate
sum(rate(airest_guardrail_blocks[5m])) by (airest_guardrail_module)
```

Import [`grafana-dashboard.json`](grafana-dashboard.json) in Grafana (Dashboards → Import). Point the Prometheus datasource variable at your OTLP → Prometheus pipeline (Grafana Alloy or OpenTelemetry Collector `prometheus` exporter).

## Datadog

With OTLP intake enabled, create widgets on:

- `airest.requests.total` — timeseries, group by `airest.endpoint`
- `airest.cache.hit` / `airest.cache.miss` — formula for hit rate
- `gen_ai.usage.input_tokens` + `gen_ai.usage.output_tokens` — cost attribution by model
- `airest.guardrail.blocks` — top blocking modules

## Traces

Key spans: `airest.request`, `airest.validate_input`, `airest.render_prompt`, `airest.llm.complete`, `airest.cache.lookup`, `airest.guardrails`, `airest.mcp.tool`, `airest.hook`.

Filter by `airest.endpoint` and `airest.request_id` to debug a single call end-to-end.
