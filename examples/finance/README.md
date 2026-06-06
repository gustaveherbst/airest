# Finance example pack

Demonstrates **payment fraud** guardrails:

- Request size cap
- Deno `payment-amount-limit` blocks amounts above `maxAmount` (TypeScript in `guardrails/payment-amount-limit.ts`, transpiled at load)
- `regex-block` on rendered prompt blocks credential-like patterns

```bash
airest validate --folder ./examples/finance
```

Try a blocked amount:

```bash
curl -X POST http://localhost:3300/v1/check-payment-fraud \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{"amount":999999,"currency":"USD","merchantId":"m1","cardLast4":"4242"}'
```

Expect HTTP 422 with `GUARDRAIL_VIOLATION`.
