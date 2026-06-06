# Production deployment

## Required environment

```bash
AIREST_PRODUCTION=true
AIREST_API_KEY=<strong-secret>          # or gateway/JWT auth on every endpoint
AIREST_HOT_RELOAD=false                 # default; never enable in prod
OPENAI_API_KEY=<real-key>               # for endpoints using provider: openai

# HTTP hardening (defaults shown)
AIREST_MAX_REQUEST_BODY_BYTES=1048576
AIREST_REQUEST_TIMEOUT_SECS=120
AIREST_GRACEFUL_SHUTDOWN=false          # opt-in; when true, drain in-flight requests on SIGTERM
AIREST_GRACEFUL_SHUTDOWN_SECS=30
AIREST_MAX_CONCURRENT_REQUESTS=64

# LLM circuit breaker
AIREST_LLM_CIRCUIT_BREAKER_THRESHOLD=5
AIREST_LLM_CIRCUIT_BREAKER_RESET_SECS=30
```

`AIREST_PRODUCTION=true` rejects placeholder secrets (`replace-me`, etc.) and refuses to start with hot reload enabled.

## Secrets injection

Inject secrets via your platform — **not** baked into images:

| Platform | Pattern |
|----------|---------|
| Kubernetes | `env` from `Secret` + `secretKeyRef` |
| Docker Compose | `env_file` or `${VAR}` from host env |
| systemd | `EnvironmentFile=/etc/airest/env` |

Minimum secrets: `AIREST_API_KEY`, provider keys (`OPENAI_API_KEY`, …), optional `AIREST_REDIS_URL` for JWT `jti` denylist.

## Health probes (Kubernetes)

```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  periodSeconds: 10
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /ready
    port: 8080
  periodSeconds: 5
  failureThreshold: 2
```

- **`/health`** — process is up; definitions loaded.
- **`/ready`** — can serve traffic: endpoints present, provider credentials valid, circuit breakers closed, not shutting down.

During graceful shutdown (SIGTERM/Ctrl+C) when **`AIREST_GRACEFUL_SHUTDOWN=true`** (opt-in; default off), `/ready` returns **503** while `/health` may still return 200 until the process exits.

### LLM provider availability — no periodic probe required

aiREST does **not** need a background job that pings OpenAI/Anthropic/etc. on a schedule. Availability is handled reactively:

| Mechanism | What it does |
|-----------|----------------|
| **Per-request errors** | Failed LLM calls return `MODEL_PROVIDER_ERROR` (HTTP 502) with provider details; endpoints can override via `errors.modelProvider.message`. |
| **Circuit breaker** | After `AIREST_LLM_CIRCUIT_BREAKER_THRESHOLD` consecutive failures per provider, new requests fail fast until `AIREST_LLM_CIRCUIT_BREAKER_RESET_SECS` elapses. |
| **`/ready`** | Fails when required provider credentials are missing or a circuit breaker is open — orchestrators stop routing traffic without burning tokens on probe calls. |

`GET {path}/health` on each endpoint confirms the **definition is loaded**, not that the upstream LLM is reachable. That is intentional: a synthetic LLM ping on every probe would add cost, rate-limit risk, and false negatives during brief provider blips. Real traffic (or your integration tests) is the authoritative signal.

If you need synthetic checks, run them **outside** aiREST (e.g. a cron `airest test my-endpoint`) rather than baking provider pings into the server process.

## CLI

```bash
# Production serve (hot reload off unless --hot-reload)
airest serve --dir ./api --production

# Load secrets from a file (values in the file override shell env for that key)
airest serve --env-file /etc/airest/env --dir ./api --production

# Dev only
airest serve --dir ./examples --hot-reload
```

## Endpoint guardrails

Every production endpoint should declare YAML guardrails. Baseline pack:

```yaml
guardrails:
  - module: max-request-size
    hook: preInput
    config:
      maxBytes: 65536
  - module: regex-block
    hook: preLlm
    config:
      target: prompt
      patterns:
        - "(?i)ignore (all )?(previous|above) instructions"
  - module: output-secret-scan
    hook: postLlm
    config: {}
```

All bundled `examples/` endpoints include this baseline or domain-specific extensions.

## MCP tool allowlists

When an endpoint declares `tools.mcpServers`, it **must** also declare a non-empty `tools.allow` list. Each entry uses qualified form `serverName/tool_name` (e.g. `huggingface/hf_doc_search`).

```yaml
tools:
  mcpServers:
    - name: huggingface
      transport: http
      url: https://hf.co/mcp
  allow:
    - huggingface/hf_doc_search
```

Remote `headers` values MAY use `${ENV_VAR}` placeholders; the reference runtime omits a header when any placeholder is unset. For HTTP MCP servers that require session affinity (e.g. `https://hf.co/mcp`), the client captures `mcp-session-id` from `initialize` and sends it on subsequent JSON-RPC calls.

**Callers never talk to MCP directly.** MCP servers are an internal integration surface; aiREST is the trust boundary. Clients use the REST route only; the runtime connects to MCP, filters tools via `tools.allow`, and invokes allowed tools on behalf of the request.

## Deno hooks — least privilege

Hook scripts (`hooks.preRequest`, `postInput`, `preLlm`, `postOutput`) run in a Deno sandbox. Treat hook source like application code: review on every change.

| Rule | Detail |
|------|--------|
| Default | `permissions: []` — no network, no filesystem |
| Network | Only when the hook calls `host.fetch()`; declare explicit hosts (`network:api.example.com`), never `network:*` |
| Guardrail scripts | `examples/guardrails/*.ts` use `evaluate(ctx)` only; `.ts` is transpiled at load; `fetch` and `Deno.read*` are rejected at load time |

See `examples/support/ticket-triage.yaml` for a no-network `postInput` hook example.

## Logging redaction on PII endpoints

Endpoints handling customer, clinical, financial, or contract data should set:

```yaml
policies:
  logRequests: true
  logResponses: false
  redactInputs: true
  redactOutputs: true
```

When `redactInputs` / `redactOutputs` are `true`, structured logs show `[REDACTED]` instead of request or model payloads. Pair with `pii-redact` guardrails where fields must be masked before the LLM, not only in logs.

## Per-endpoint YAML safety levers

Per-endpoint YAML is the primary safety surface. Review every production endpoint against this checklist:

### Guardrails (sensitive APIs)

| Module | Hook | Use when |
|--------|------|----------|
| `max-request-size` | `preInput` | Always — cap request body size |
| `pii-redact` | `postInput` | Customer, clinical, contract, or payment payloads |
| `topic-allowlist` / custom Deno | `postInput` | Domain-specific topic restrictions |
| `regex-block` | `preLlm` | Block prompt-injection patterns |
| `output-secret-scan` | `postLlm` | Scan model output for secrets before returning |

### Policies — cost and tool caps

```yaml
policies:
  maxRetries: 2          # default; lower for expensive models
  maxToolRounds: 5       # default; lower for MCP/agentic endpoints
  toolTimeoutMs: 10000   # default; prefer ≤10s for MCP tools
```

`kb-ticket-search-hf.yaml` demonstrates tighter caps (`maxRetries: 1`, `maxToolRounds: 4`, `toolTimeoutMs: 10000`) for tool-heavy routes.

### Cache

```yaml
cache:
  enabled: false
```

Set explicitly on **non-deterministic** endpoints (MCP tools, high-temperature creative APIs) and **highly sensitive** endpoints (healthcare, finance, legal, support). Omitting `cache` is equivalent to disabled; explicit `false` documents intent in review.

### Telemetry

```yaml
telemetry:
  enabled: false
```

Disable on endpoints that must not export traces to your OTEL backend (PHI, PCI, privileged legal review). Defaults to enabled when omitted; set `false` deliberately where trace export is a compliance risk.

### Model provider errors

Return a caller-safe message when the LLM is down — no need for a separate liveness probe:

```yaml
errors:
  modelProvider:
    message: The analysis service is temporarily unavailable. Please retry shortly.
```

## Architecture

Place aiREST behind Kong/Envoy for TLS and rate limiting. See [`examples/gateway/README.md`](../../examples/gateway/README.md).
