# aiREST Example APIs

This directory contains **18** example endpoint definitions across **8 categories**, including healthcare, finance, auth strategies, **local Deno tools**, and **MCP tool-calling** (remote HF HTTP, mock HTTP/SSE, and local tools).

**One YAML file = one aiREST API.** Filenames are for organization only; API identity comes from `name`, `category`, and `path` inside each file.

Copy [`.env.example`](../.env.example) to a local `.env` with `OPENAI_API_KEY`. Pass it explicitly with `--env-file` so shell exports do not override file values.

## Folder layout

| Folder | APIs | Load command |
|--------|------|--------------|
| `legal/` | Contract risk analyzer, NDA risk check | `airest serve --folder ./examples/legal` |
| `analytics/` | Sentiment (POST), quick sentiment (GET), text summarizer | `airest serve --folder ./examples/analytics` |
| `healthcare/` | Clinical note summary (PII redact + topic allowlist) | `airest serve --folder ./examples/healthcare` |
| `finance/` | Payment fraud check (amount limit + secret scan) | `airest serve --folder ./examples/finance` |
| `content/` | Headline generator | `airest serve --folder ./examples/content` |
| `support/` | Ticket triage, reply suggester, escalation advisor | `airest serve --folder ./examples/support` |
| `mcp/` | **Public HF MCP**, mock HTTP/SSE, local Deno tool | `airest serve --folder ./examples/mcp --no-recursive` |
| `auth/` | JWT, OAuth2 introspect, trust-gateway | `airest serve --folder ./examples/auth` |
| `guardrails/` | Shared TypeScript modules (`path:` from YAML) | — |
| `gateway/` | Kong/Envoy snippets for gateway auth | — |
| `./` (all) | All 18 examples | `airest serve --folder ./examples` |

## MCP examples

| Transport | File | REST route |
|-----------|------|------------|
| `http` (HF, public) | `mcp/kb-ticket-search-hf.yaml` | `POST /v1/search-support-kb` |
| `local` (Deno) | `mcp/kb-ticket-search-local.yaml` | `POST /v1/search-support-kb-local` |
| `http` (mock) | `mcp/kb-ticket-search-http.yaml` | `POST /v1/search-support-kb-http` |
| `sse` (mock) | `mcp/kb-ticket-search-sse.yaml` | `POST /v1/search-support-kb-sse` |

The **HF** example (`mcp/kb-ticket-search-hf.yaml`) calls `https://hf.co/mcp` directly — no local mock required. See **[mcp/README.md](mcp/README.md)**.

The **mock HTTP/SSE** examples in `mcp/` require the local mock server:

```bash
node examples/mcp/mcp-mock-kb-remote.mjs   # port 3100
airest serve --folder ./examples/mcp
```

## Smoke-test all examples

```bash
./examples/runall.sh
```

Prompts for your `.env` path, starts aiREST per folder with `--env-file`, curls all 18 endpoints (starts the MCP mock when testing `mcp/`), and reports pass/fail/skip.

See **[mcp/README.md](mcp/README.md)** for transport details. Callers never talk to MCP directly — only aiREST REST routes.

## Validate

```bash
airest validate --folder ./examples
airest validate --file ./examples/legal/contract-risk-analyzer.yaml
airest validate --folder ./examples/support
airest validate --folder ./examples/mcp
```

## GET vs POST input

- **POST** endpoints accept JSON in the request body (`inputSchema` validates the body).
- **GET** endpoints accept input via query parameters (`?text=hello` maps to `inputSchema.properties.text`).

## Health checks

- **Server:** `GET /health` — lists all loaded endpoints
- **Readiness:** `GET /ready` — credentials, circuit breakers, shutdown state
- **Per API:** `GET {path}/health` — e.g. `GET /v1/analyze-contract-risk/health`

## Guardrails, hooks, cache

Several endpoints declare `guardrails:` (built-in + Deno `path:` TypeScript modules in `guardrails/`, transpiled at load). `support/ticket-triage.yaml` demonstrates a Deno `postInput` hook with `permissions: []`. Sensitive categories set `cache.enabled: false` and log redaction policies.

Each category folder includes a `README.md` with curl examples. Normative schema: **[SCHEMA.md](../SCHEMA.md)**. Production checklist: **[docs/deploy/production.md](../docs/deploy/production.md)**.
