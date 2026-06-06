# Support — API Bundle (3 endpoints)

This folder demonstrates loading **multiple aiREST APIs from one directory**. All three YAML files are registered when you point aiREST at this folder (recursive scan is on by default).

| File | aiREST API name | REST route |
|------|-----------------|------------|
| `ticket-triage.yaml` | `support-ticket-triage` | `POST /v1/triage-support-ticket` |
| `reply-suggester.yaml` | `support-reply-suggester` | `POST /v1/suggest-support-reply` |
| `escalation-advisor.yaml` | `support-escalation-advisor` | `POST /v1/advise-support-escalation` |

Filenames differ from API names on purpose — identity comes from the YAML `name` field.

For **MCP tool-calling** (public Hugging Face server, local mock HTTP/SSE, local Deno tools), see **[`../mcp/README.md`](../mcp/README.md)** — especially **`kb-ticket-search-hf.yaml`**.

## Load all three from this folder

```bash
airest serve --env-file ./.env --folder ./examples/support
```

Expected startup (order may vary):

```text
Registered POST /v1/advise-support-escalation [support/support-escalation-advisor]
Registered POST /v1/suggest-support-reply [support/support-reply-suggester]
Registered POST /v1/triage-support-ticket [support/support-ticket-triage]
```

## Validate the folder

```bash
airest validate --folder ./examples/support
```

## Health checks

```bash
curl http://localhost:3300/v1/triage-support-ticket/health
curl http://localhost:3300/v1/advise-support-escalation/health
curl http://localhost:3300/v1/suggest-support-reply/health
curl http://localhost:3300/health
```

## Call ticket triage

```bash
curl -X POST http://localhost:3300/v1/triage-support-ticket \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{
    "subject": "Cannot access my account",
    "body": "I reset my password twice and still cannot log in.",
    "customerTier": "premium"
  }'
```

## Call reply suggester

```bash
curl -X POST http://localhost:3300/v1/suggest-support-reply \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{
    "customerMessage": "My invoice looks wrong and I was charged twice this month.",
    "tone": "empathetic"
  }'
```

## Call escalation advisor

```bash
curl -X POST http://localhost:3300/v1/advise-support-escalation \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{
    "subject": "Production outage affecting billing",
    "body": "Our checkout has been failing for two hours.",
    "priority": "critical",
    "customerTier": "enterprise",
    "previousAttempts": 1
  }'
```

## Deno hooks

`ticket-triage.yaml` includes a `postInput` hook with `permissions: []` (no network). Hooks that call `host.fetch()` must declare explicit `network:host` permissions; `network:*` is rejected at validate time.

## Test with CLI

```bash
airest test support-ticket-triage --folder ./examples/support
airest test support-reply-suggester --folder ./examples/support
airest test support-escalation-advisor --folder ./examples/support
```
