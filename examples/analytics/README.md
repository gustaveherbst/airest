# Analytics — Sentiment & Summarization

Three aiREST APIs for text analysis (POST and GET).

| File | API name | REST route |
|------|----------|------------|
| `sentiment-analyzer.yaml` | `sentiment-analyzer` | `POST /v1/analyze-sentiment` |
| `quick-sentiment.yaml` | `quick-sentiment` | `GET /v1/quick-sentiment` |
| `text-summarizer.yaml` | `text-summarizer` | `POST /v1/summarize-text` |

## Load this folder

```bash
airest serve --folder ./examples/analytics
```

## Sentiment analyzer (POST)

```bash
curl -X POST http://localhost:3300/v1/analyze-sentiment \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{"text": "I love this product!"}'
```

```bash
airest test sentiment-analyzer --folder ./examples/analytics
```

## Quick sentiment (GET)

Input is passed as query parameters instead of a JSON body:

```bash
curl -G "http://localhost:3300/v1/quick-sentiment" \
  --data-urlencode "text=I love this product!" \
  -H "x-api-key: $AIREST_API_KEY"
```

```bash
airest test quick-sentiment --folder ./examples/analytics
```

## Text summarizer (POST)

```bash
curl -X POST http://localhost:3300/v1/summarize-text \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{
    "text": "Quarterly revenue grew 12 percent driven by enterprise subscriptions while support costs increased due to onboarding volume.",
    "maxBullets": 3,
    "audience": "executive"
  }'
```

```bash
airest test text-summarizer --folder ./examples/analytics
```
