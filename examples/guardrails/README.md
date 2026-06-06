# Shared guardrail scripts

TypeScript modules loaded by endpoint YAML via `runtime: deno` and `path:` (relative to the endpoint YAML file). aiREST transpiles `.ts` files to JavaScript (via SWC) before executing them in the embedded V8 sandbox.

Ambient types for `GuardrailEvaluateContext` and `GuardrailEvaluateResult` are provided automatically — see `guardrails/runtime/guardrail-types.ts`.

Each module must export `function evaluate(ctx)` returning:

| `action` | Meaning |
|----------|---------|
| `pass` | Continue the pipeline |
| `block` | Reject with `message` (HTTP 422 guardrail violation) |
| `modify` | Replace `input` and/or `output` JSON |
| `warn` | Log warning, continue |

Sandbox: `fetch`, `Deno.read*`, `Deno.write*`, and similar APIs are rejected at load time.

**Review guardrail scripts like application code.** They run on every matching request; keep logic minimal and side-effect free. For outbound HTTP, use **hooks** (`host.fetch` with explicit `network:host` permissions) — not guardrail `evaluate(ctx)` modules.

## Scripts

| File | Used by |
|------|---------|
| `clinical-topic-allowlist.ts` | `healthcare/clinical-note-summary.yaml` |
| `payment-amount-limit.ts` | `finance/payment-fraud-check.yaml` |
| `support-priority-cap.ts` | `support/ticket-triage.yaml` |

See also built-in modules in [SCHEMA.md](../../SCHEMA.md): `max-request-size`, `pii-redact`, `regex-block`.
