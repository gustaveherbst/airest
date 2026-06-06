# Healthcare example pack

Demonstrates **HIPAA-oriented guardrails** on a clinical summarization endpoint:

- `max-request-size` on `preInput`
- `pii-redact` masks `patientName`, `mrn`, `dateOfBirth` before the LLM
- TypeScript `clinical-topic-allowlist` blocks disallowed clinical topics (transpiled at load from `guardrails/clinical-topic-allowlist.ts`)

```bash
airest validate --folder ./examples/healthcare
airest serve --folder ./examples/healthcare
```

```bash
curl -X POST http://localhost:3300/v1/summarize-clinical-note \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{"clinicalNote":"Patient stable after therapy.","patientName":"Jane Doe","mrn":"12345","dateOfBirth":"1980-01-01"}'
```
