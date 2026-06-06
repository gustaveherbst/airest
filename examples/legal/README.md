# Legal — Contract & NDA APIs

Two aiREST APIs for legal document analysis.

| File | API name | REST route |
|------|----------|------------|
| `contract-risk-analyzer.yaml` | `contract-risk-analyzer` | `POST /v1/analyze-contract-risk` |
| `nda-risk-check.yaml` | `nda-risk-check` | `POST /v1/check-nda-risk` |

## Load this folder

```bash
airest serve --folder ./examples/legal
```

## Validate

```bash
airest validate --folder ./examples/legal
```

## Contract risk analyzer

```bash
curl -X POST http://localhost:3300/v1/analyze-contract-risk \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{
    "contractText": "This agreement is entered into between Company A and Company B and includes payment terms, termination clauses, and confidentiality obligations sufficient for review.",
    "jurisdiction": "Oklahoma",
    "riskTolerance": "medium"
  }'
```

```bash
airest test contract-risk-analyzer --folder ./examples/legal
```

## NDA risk check

```bash
curl -X POST http://localhost:3300/v1/check-nda-risk \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{
    "ndaText": "This mutual non-disclosure agreement governs confidential information shared between the parties for evaluating a potential business relationship and includes term, exclusions, and return obligations.",
    "partyRole": "mutual",
    "jurisdiction": "Delaware"
  }'
```

```bash
airest test nda-risk-check --folder ./examples/legal
```
