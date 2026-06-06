# Content — Headline Generator

**aiREST API:** `headline-generator`  
**Category:** `content`  
**REST route:** `POST /v1/generate-headline`

Generates marketing headline options from a product brief.

## Load this folder

```bash
airest serve --folder ./examples/content
```

## Call the API

```bash
curl -X POST http://localhost:3300/v1/generate-headline \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{
    "product": "aiREST turns YAML into production AI REST APIs",
    "audience": "backend engineers",
    "tone": "professional",
    "count": 3
  }'
```

## Test with CLI

```bash
airest test headline-generator --folder ./examples/content
```
