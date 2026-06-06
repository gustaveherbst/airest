# aiREST Endpoint Definition Schema

| | |
|---|---|
| **Schema version** | `1.0.0` |
| **Document version** | `1.0.0` |
| **Status** | Draft standard |
| **Date** | 2026-06-04 |
| **Reference implementation** | [aiREST](https://github.com/gustaveherbst/airest) (Rust runtime) |
| **Media type** | `application/yaml` |
| **File extensions** | `.yaml`, `.yml` |

This document normatively defines the **aiREST endpoint definition** format: a declarative YAML file that describes one AI-powered REST API — its input contract, prompts, output contract, model configuration, and runtime policies.

Implementations that claim aiREST Definition Schema **1.0.0** conformance MUST accept files conforming to this specification and MUST produce HTTP behavior consistent with the runtime semantics described herein.

---

## 1. Scope

An aiREST endpoint definition specifies:

- How clients call the API (`method`, `path`, `auth`)
- What JSON clients may send (`inputSchema`)
- How the model is instructed (`systemPrompt`, `userPromptTemplate`)
- What JSON the API MUST return (`outputSchema`)
- Which LLM provider and model to use (`model`)
- Validation, retry, logging, and tool-loop behavior (`policies`)
- Optional safety modules (`guardrails`), Deno hooks (`hooks`), agentic tools (`tools` — MCP + local), semantic cache (`cache`), and telemetry export (`telemetry`)

**One YAML file = one REST endpoint.**

The filename is not normative. Endpoint identity is defined by fields inside the file (`name`, `category`, `path`).

This schema describes **definition files only**. The companion HTTP response envelope (§12) is part of the aiREST REST contract but is not stored in YAML.

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Definition** | A single YAML file conforming to this schema |
| **Endpoint** | A running REST route materialized from a definition |
| **Input** | JSON request body validated against `inputSchema` |
| **Output** | JSON response body validated against `outputSchema` |
| **Provider** | LLM backend selected by `model.provider` |
| **Runtime** | Engine that loads definitions and serves HTTP (e.g. aiREST) |

Keywords **MUST**, **SHOULD**, **MAY** are used in the RFC 2119 sense.

---

## 3. File format

### 3.1 Syntax

- Definitions MUST be valid YAML 1.2.
- JSON definition files are NOT part of schema 1.0.0.
- Field names MUST use **camelCase** as specified in this document (e.g. `inputSchema`, not `input_schema`).

### 3.2 Discovery

Runtimes MAY load definitions from a directory tree. The reference implementation loads `.yaml` and `.yml` files recursively unless configured otherwise.

Within a loaded set:

- `name` MUST be unique
- the combination of `method` + `path` MUST be unique

### 3.3 Versioning

| Field | Scope |
|-------|--------|
| **Schema version** (this document) | Format of the YAML file |
| **`version`** (in YAML) | Semantic version of a single endpoint definition |

Breaking changes to this document increment the schema major version (e.g. `2.0.0`). Runtimes SHOULD reject or warn on unsupported schema versions when an explicit schema declaration is added in a future revision.

---

## 4. Top-level object

Every definition MUST be a YAML mapping with the following fields.

### 4.1 Field summary

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `name` | string | **yes** | — |
| `version` | string | **yes** | — |
| `description` | string | no | omitted |
| `category` | string | no | omitted |
| `method` | string | **yes** | — |
| `path` | string | **yes** | — |
| `auth` | object | no | auth disabled |
| `inputSchema` | object | **yes** | — |
| `systemPrompt` | string | **yes** | — |
| `userPromptTemplate` | string | no | input JSON serialized to prompt |
| `outputSchema` | object | **yes** | — |
| `model` | object | **yes** | — |
| `policies` | object | no | see §10 |
| `guardrails` | array | no | see §15 |
| `hooks` | object | no | see §16 |
| `tools` | object | no | see §17 (MCP + local tools) |
| `cache` | object | no | see §18 |
| `telemetry` | object | no | see §19 |
| `errors` | object | no | framework defaults |
| `examples` | object | no | omitted |
| `health` | object | no | default health response |

### 4.2 Complete skeleton

```yaml
name: my-endpoint
category: analytics
version: 1.0.0
description: Human-readable summary of what this API does.

method: POST
path: /v1/my-endpoint

auth:
  required: true

inputSchema:
  type: object
  additionalProperties: false
  required: [text]
  properties:
    text:
      type: string
      minLength: 1

systemPrompt: |
  You are an expert assistant.
  Return only valid JSON matching the output schema.

userPromptTemplate: |
  Text:
  {{text}}

outputSchema:
  type: object
  additionalProperties: false
  required: [result]
  properties:
    result:
      type: string

model:
  provider: openai
  model: gpt-4.1-mini
  temperature: 0.2
  maxTokens: 1000

policies:
  validateInput: true
  validateOutput: true
  retryOnInvalidJson: true
  retryOnInvalidSchema: true
  maxRetries: 2
  maxToolRounds: 5
  toolTimeoutMs: 10000
  stripMarkdownCodeFences: true
  logRequests: true
  logResponses: false
  redactInputs: false
  redactOutputs: false

guardrails:
  - module: max-request-size
    hook: preInput
    config:
      maxBytes: 65536

hooks:
  postInput:
    runtime: deno
    permissions: []
    script: |
      function transform(input, host) { return input; }

tools:
  local:
    - name: search_kb
      description: Search KB
      inputSchema:
        type: object
        required: [query]
        properties:
          query: { type: string }
      runtime: deno
      path: ./tools/search-kb.ts
  allow:
    - local/search_kb

cache:
  enabled: false

telemetry:
  enabled: true

health:
  message: my-endpoint is loaded and ready.
  status: 200

errors:
  inputValidation:
    message: Custom input validation message.

examples:
  request:
    text: Example input
  response:
    result: Example output
```

---

## 5. Identity and HTTP routing

### 5.1 `name`

- **Type:** string  
- **Required:** yes  
- **Constraints:** non-empty after trim  
- **Semantics:** Logical API identifier. Returned in successful responses as `meta.endpoint`. Independent of the YAML filename.

```yaml
name: sentiment-analyzer
```

### 5.2 `version`

- **Type:** string  
- **Required:** yes  
- **Constraints:** non-empty after trim  
- **Semantics:** Semantic version of this definition (e.g. `1.0.0`). Returned in `meta.version`.

### 5.3 `description`

- **Type:** string  
- **Required:** no  
- **Semantics:** Human-readable summary. Used in OpenAPI generation and catalogs.

### 5.4 `category`

- **Type:** string  
- **Required:** no  
- **Semantics:** Grouping label for catalogs, CLI output, OpenAPI tags, and logs (e.g. `legal`, `support`, `analytics`).

### 5.5 `method`

- **Type:** string  
- **Required:** yes  
- **Constraints:** Schema 1.0.0 supports **`GET`** and **`POST`** (case-insensitive at load time; stored as provided).

| Method | Input source | Notes |
|--------|--------------|-------|
| **`POST`** | JSON request body | Standard for rich or nested input objects |
| **`GET`** | URL query parameters | Input mapped from query string to `inputSchema` top-level properties |

GET endpoints MUST NOT rely on a request body. Query parameter names MUST match top-level property names in `inputSchema`.

### 5.6 `path`

- **Type:** string  
- **Required:** yes  
- **Constraints:**
  - non-empty after trim
  - MUST start with `/`
  - the combination of `method` + `path` MUST be unique among loaded definitions
- **Semantics:** HTTP path exposed by the runtime.

```yaml
path: /v1/analyze-sentiment
```

**Derived route:** per-endpoint health is served at `{path}/health` (see §11).

---

## 6. Authentication (`auth`)

### 6.1 Object shape

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `required` | boolean | yes (if `auth` present) | — |
| `type` | string | no | `apiKey` |
| `jwt` | object | when `type: jwt` | — |
| `oauth2` | object | when `type: oauth2Introspect` | — |
| `trustGateway` | object | when `type: trustGateway` | — |

```yaml
auth:
  required: true
  type: apiKey
```

If `auth` is omitted, authentication is disabled for the endpoint.

### 6.2 `auth.type` values

| Value | Caller credential | YAML block |
|-------|-------------------|------------|
| `apiKey` | Header `x-api-key` | (default) |
| `jwt` | `Authorization: Bearer <JWT>` | `auth.jwt` |
| `oauth2Introspect` | Bearer access token | `auth.oauth2` |
| `trustGateway` | Gateway identity headers | `auth.trustGateway` |
| `none` | No check | — |

### 6.3 `auth.jwt`

| Field | Type | Description |
|-------|------|-------------|
| `issuer` | string | Expected JWT `iss` |
| `audience` | string | Expected JWT `aud` |
| `jwksUrl` | string | JWKS URL for signature verification |
| `algorithms` | string[] | Allowed algs (default RS256) |
| `claims.required` | string[] | Required claim names |
| `claims.scope` | string | Required OAuth scope |

Environment fallbacks: `AIREST_JWT_JWKS_URL`, `AIREST_JWT_ISSUER`, `AIREST_JWT_AUDIENCE`.

### 6.4 `auth.oauth2`

| Field | Type | Description |
|-------|------|-------------|
| `url` | string | Token introspection endpoint |
| `clientId` | string | OAuth client id |
| `clientSecret` | string | OAuth client secret |

Environment fallbacks: `AIREST_OAUTH2_INTROSPECTION_URL`, `AIREST_OAUTH2_CLIENT_ID`, `AIREST_OAUTH2_CLIENT_SECRET`.

### 6.5 `auth.trustGateway`

| Field | Type | Default |
|-------|------|---------|
| `userIdHeader` | string | `x-user-id` |
| `tenantIdHeader` | string | optional |

Used when an upstream gateway (Kong, Envoy) terminates auth and forwards identity headers.

### 6.6 Runtime semantics

When `auth.required` is `true` and `type` is `apiKey`:

- Runtimes MUST validate `x-api-key` against a configured server secret when that secret is set.
- If the server secret is unset or blank, conforming runtimes MAY skip enforcement (development mode).

JWT `jti` revocation MAY use `AIREST_JTI_DENYLIST` or Redis (`AIREST_REDIS_URL`).

See also §20 for the same auth types in summary form.

---

## 7. JSON Schema contracts

### 7.1 `inputSchema`

- **Type:** JSON Schema object  
- **Required:** yes  
- **Constraints:**
  - MUST be a JSON object (not an array or scalar)
  - MUST declare `"type": "object"`

Validates client input **before** any LLM call.

| HTTP method | Input source |
|-------------|--------------|
| `POST` | JSON request body |
| `GET` | URL query parameters coerced to JSON using `inputSchema.properties` |

For **GET**, runtimes MUST:

1. Parse the query string into a flat JSON object keyed by parameter name
2. Coerce values using each property's JSON Schema `type` (`string`, `integer`, `number`, `boolean`, `array`, `object`)
3. Validate the resulting object against `inputSchema`

Query parameter rules:

- Parameter names MUST match top-level `inputSchema` property names
- **Arrays:** repeat the parameter (`?tag=a&tag=b`) or use comma-separated values (`?tag=a,b`) when a single value is provided
- **Objects:** pass a URL-encoded JSON object as the parameter value
- Omitted optional properties are absent from the input object (not `null`)
- GET requests MUST NOT require a request body

**Standard:** JSON Schema **Draft 7** semantics. Runtimes SHOULD use a Draft 7–compatible validator.

### 7.2 `outputSchema`

- **Type:** JSON Schema object  
- **Required:** yes  
- **Constraints:** same as `inputSchema`

Validates model output **after** JSON parsing and **before** the success response is returned.

### 7.3 Recommended practice

Definitions SHOULD set `"additionalProperties": false` on both schemas for strict contracts.

Example:

```yaml
inputSchema:
  type: object
  additionalProperties: false
  required: [text]
  properties:
    text:
      type: string
      minLength: 1
```

---

## 8. Prompts

### 8.1 `systemPrompt`

- **Type:** string (multiline supported)  
- **Required:** yes  
- **Constraints:** non-empty after trim  
- **Semantics:** System-level instruction sent to the model.

```yaml
systemPrompt: |
  You are a senior analyst.
  Return only valid JSON matching the output schema.
  Do not include markdown or prose outside the JSON object.
```

### 8.2 `userPromptTemplate`

- **Type:** string  
- **Required:** no  
- **Semantics:** Handlebars template rendered with the validated input object as context.

```yaml
userPromptTemplate: |
  Contract:
  {{contractText}}

  Jurisdiction:
  {{jurisdiction}}
```

**Template engine:** [Handlebars](https://handlebarsjs.com/) syntax. Missing optional input fields render as empty strings.

**If omitted:** the runtime MUST serialize the validated input JSON (pretty-printed) as the user message body.

### 8.3 Schema instruction (runtime)

Conforming runtimes MUST append an instruction to the user message requiring JSON output conforming to `outputSchema`. This behavior is implicit and not configured in YAML.

---

## 9. Model configuration (`model`)

### 9.1 Object shape

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `provider` | string | **yes** | — |
| `model` | string | **yes** | — |
| `temperature` | number | no | provider default |
| `maxTokens` | integer | no | provider default |

```yaml
model:
  provider: openai
  model: gpt-4.1-mini
  temperature: 0
  maxTokens: 2000
```

### 9.2 `provider`

**Type:** string  
**Required:** yes  

Schema 1.0.0 normative values and aliases:

| Value | Aliases | Description |
|-------|---------|-------------|
| `openai` | — | OpenAI-compatible chat completions |
| `azure_openai` | `azure`, `azure-openai` | Azure OpenAI deployments |
| `anthropic` | — | Anthropic Messages API |
| `gemini` | `google` | Google Gemini |
| `grok` | `xai` | xAI Grok (OpenAI-compatible) |
| `ollama` | — | Local Ollama (OpenAI-compatible) |

Comparison is case-insensitive. Runtimes SHOULD normalize to the canonical value in logs and telemetry.

**Credentials:** Provider API keys and base URLs MUST NOT appear in definition files. Runtimes load credentials from environment variables or secure configuration stores.

For Azure OpenAI, `model` is the **deployment name**, not necessarily the underlying model id.

### 9.3 `model`

- **Type:** string  
- **Required:** yes  
- **Constraints:** non-empty after trim  
- **Semantics:** Provider-specific model or deployment identifier.

---

## 10. Runtime policies (`policies`)

Controls validation, retries, logging, and output parsing. All fields are optional; defaults below apply when `policies` is omitted.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `validateInput` | boolean | `true` | Validate request body against `inputSchema` |
| `validateOutput` | boolean | `true` | Validate model JSON against `outputSchema` |
| `retryOnInvalidJson` | boolean | `true` | Retry when model output is not parseable JSON |
| `retryOnInvalidSchema` | boolean | `true` | Retry when parsed JSON fails output schema |
| `maxRetries` | integer | `2` | Maximum retry attempts after initial call |
| `stripMarkdownCodeFences` | boolean | `true` | Strip ` ```json ` fences before JSON parse |
| `logRequests` | boolean | `true` | Log request processing (subject to redaction) |
| `logResponses` | boolean | `false` | Log model outputs (subject to redaction) |
| `redactInputs` | boolean | `false` | Log `[REDACTED]` instead of input payloads |
| `redactOutputs` | boolean | `false` | Log `[REDACTED]` instead of output payloads |
| `maxToolRounds` | integer | `5` | Maximum tool-loop rounds (MCP + local tools) per request |
| `toolTimeoutMs` | integer | `10000` | Per-tool call timeout in milliseconds |

### 10.1 Retry semantics

When a retry policy triggers and `maxRetries` has not been exhausted, the runtime MUST:

1. Send a correction prompt including validation or parse errors and the required output schema
2. Re-invoke the model
3. Re-validate the result

If retries are exhausted, the runtime MUST return an error response (§12.2).

---

## 11. Health checks (`health`)

Optional configuration for **`GET {path}/health`**.

| Field | Type | Required | Default |
|-------|------|----------|---------|
| `message` | string | no | `"{name} is healthy"` |
| `status` | integer | no | `200` |

**Constraints:**

- `status` MUST be between 200 and 599 inclusive
- `message` MUST NOT be empty when provided

```yaml
health:
  message: Contract risk analyzer is loaded and ready.
  status: 200
```

Health routes MUST NOT require authentication.

Runtimes SHOULD expose:

| Route | Semantics |
|-------|-----------|
| `GET /health` | Process is up; lists loaded endpoints (reference) |
| `GET /ready` | Ready for traffic: definitions loaded, provider credentials valid, circuit breakers closed, not shutting down (reference) |
| `GET {path}/health` | Per-endpoint definition health (§11) |

---

## 12. HTTP response contract

Definitions imply the following JSON response shapes for endpoint invocations. Field names use **camelCase**.

### 12.1 Success (`2xx`)

Returned when input validation, model execution, and output validation succeed.

```json
{
  "success": true,
  "data": { },
  "meta": {
    "requestId": "req_abc123",
    "endpoint": "sentiment-analyzer",
    "version": "1.0.0",
    "model": "gpt-4.1-mini",
    "latencyMs": 820
  }
}
```

| Field | Description |
|-------|-------------|
| `success` | Always `true` |
| `data` | Validated JSON matching `outputSchema` |
| `meta.requestId` | Unique request identifier |
| `meta.endpoint` | Definition `name` |
| `meta.version` | Definition `version` |
| `meta.model` | Resolved model identifier used for the call |
| `meta.latencyMs` | Total LLM-related latency in milliseconds |

### 12.2 Error

```json
{
  "success": false,
  "error": {
    "type": "INPUT_VALIDATION_ERROR",
    "message": "Request body does not match input schema."
  },
  "meta": {
    "requestId": "req_abc123",
    "endpoint": "sentiment-analyzer",
    "version": "1.0.0"
  }
}
```

### 12.3 Standard error types

| `error.type` | Typical HTTP status | When |
|--------------|---------------------|------|
| `NOT_FOUND` | 404 | Unknown route |
| `INPUT_VALIDATION_ERROR` | 400 | Input fails `inputSchema` |
| `AUTHENTICATION_ERROR` | 401 | Missing or invalid credentials |
| `PROMPT_RENDERING_ERROR` | 500 | Template rendering failed |
| `MODEL_PROVIDER_ERROR` | 502 | LLM provider failure |
| `MODEL_JSON_PARSE_ERROR` | 502 | Model output not valid JSON |
| `MODEL_OUTPUT_VALIDATION_ERROR` | 502 | Output fails `outputSchema` |
| `INTERNAL_SERVER_ERROR` | 500 | Unexpected runtime failure |
| `ENDPOINT_DEFINITION_ERROR` | 500 | Invalid loaded definition |
| `GUARDRAIL_VIOLATION` | 403 | Guardrail module blocked or modified request |
| `HOOK_EXECUTION_ERROR` | 403 | Deno hook script failed |
| `MCP_TOOL_ERROR` | 502 | MCP server communication or tool failure |
| `CACHE_ERROR` | 500 | Cache store failure |

Runtimes MAY override `error.message`, `error.type`, and HTTP status per endpoint using the `errors` block (§13).

---

## 13. Error overrides (`errors`)

Optional per-endpoint customization of error responses.

### 13.1 Supported keys

Each key maps to an `ErrorOverride` object:

| Key | Overrides |
|-----|-----------|
| `inputValidation` | Input schema failures |
| `authentication` | Auth failures |
| `promptRendering` | Prompt template failures |
| `modelProvider` | LLM provider failures |
| `modelJsonParse` | JSON parse failures |
| `modelOutputValidation` | Output schema failures |
| `internalServer` | Internal errors |
| `guardrail` | Guardrail violation (`GUARDRAIL_VIOLATION`) |
| `hookExecution` | Hook script failure (`HOOK_EXECUTION_ERROR`) |
| `cache` | Cache store failure (`CACHE_ERROR`) |

### 13.2 `ErrorOverride` shape

| Field | Type | Required | Constraints |
|-------|------|----------|-------------|
| `message` | string | **yes** | non-empty |
| `type` | string | no | replaces standard `error.type` string |
| `status` | integer | no | HTTP status 400–599 |

```yaml
errors:
  inputValidation:
    message: Contract request is missing required fields.
  authentication:
    message: A valid x-api-key header is required.
    status: 403
    type: CUSTOM_AUTH_ERROR
  modelOutputValidation:
    message: Could not produce a valid structured result.
```

If a key is omitted, the runtime MUST use its standard message and status for that error class.

---

## 14. Examples (`examples`)

Optional sample payloads for documentation, CLI testing, and OpenAPI.

| Field | Type | Required |
|-------|------|----------|
| `request` | object | no |
| `response` | object | no |

```yaml
examples:
  request:
    text: I love this product!
  response:
    sentiment: positive
    confidence: 0.95
```

**Semantics:**

- `request` SHOULD validate against `inputSchema`
- `response` SHOULD validate against `outputSchema`
- Runtimes MAY use `examples.request` when CLI tools invoke the endpoint without an explicit input file

---

## 15. Guardrails (`guardrails`)

Optional array of guardrail modules executed at defined pipeline hooks.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `module` | string | yes | Built-in name or logical module id |
| `hook` | string | yes | `preInput`, `postInput`, `preLlm`, `postLlm`, `postOutput`, `preCacheWrite` |
| `runtime` | string | no | `builtin` (default) or `deno` |
| `script` | string | no | Inline TypeScript for `deno` runtime |
| `path` | string | no | Path to `.ts`/`.js` file relative to YAML |
| `timeoutMs` | integer | no | Execution timeout |
| `config` | object | no | Module-specific configuration |

Built-in modules: `max-request-size`, `pii-redact`, `regex-block`, `output-secret-scan`, `topic-allowlist`.

Deno guardrails MUST define `function evaluate(ctx)` returning `{ action: 'pass' | 'block' | 'modify' | 'warn', ... }`. Sandbox rejects `fetch`, `Deno.read*`, `Deno.write*`, etc. at load time.

**TypeScript (reference runtime):** When `runtime: deno` and `script` or `path` ends in `.ts` (or contains TypeScript type syntax), conforming runtimes SHOULD transpile to JavaScript before sandbox execution. The reference implementation uses SWC, resolves `path` relative to the YAML file, and prepends ambient types from `guardrails/runtime/guardrail-types.ts` so authors may use `GuardrailEvaluateContext` and `GuardrailEvaluateResult` without declaring them.

---

## 16. Hooks (`hooks`)

Optional Deno sandbox scripts at pipeline extension points.

| Key | `HookSpec` fields |
|-----|-------------------|
| `preRequest` | Runs before guardrails |
| `postInput` | After input validation |
| `preLlm` | Before LLM call |
| `postOutput` | After output validation |

Each `HookSpec`:

| Field | Type | Required |
|-------|------|----------|
| `runtime` | string | yes (`deno` or `inline`) |
| `script` | string | yes — must define `transform(input, host)` |
| `timeoutMs` | integer | no |
| `permissions` | string[] | no — e.g. `network:api.example.com`; `network:*` MUST NOT be used |

Scripts calling `host.fetch()` MUST declare matching `network:` permissions.

---

## 17. Tools (`tools`) — MCP and local

Optional agentic tool configuration. **Tool backends are internal** — HTTP clients call the aiREST REST route only; MCP URLs and local scripts are never exposed to callers.

| Field | Type | Required |
|-------|------|----------|
| `mcpServers` | array | no |
| `local` | array | no |
| `allow` | string[] | **yes when `mcpServers` or `local` is non-empty** |
| `toolTimeoutMs` | integer | no |

Each `mcpServers[]` entry:

| Field | Type | Required for transport |
|-------|------|------------------------|
| `name` | string | always |
| `transport` | string | always — `stdio`, `http`, `streamableHttp`, or `sse` |
| `command` | string | `stdio` |
| `args` | string[] | no |
| `env` | object | no |
| `url` | string | `http`, `streamableHttp`, `sse` |
| `headers` | object | no — auth headers to remote server; values MAY use `${ENV_VAR}` placeholders expanded from the process environment (reference runtime omits a header when any placeholder is unset) |

`allow` entries MUST use qualified form `serverName/tool_name` (e.g. `huggingface/hf_doc_search`) or `local/tool_name` (e.g. `local/search_kb`).

**HTTP / streamableHttp (reference runtime):** Clients SHOULD send `Accept: application/json, text/event-stream`, capture `mcp-session-id` from the `initialize` response, and include it on subsequent JSON-RPC POSTs when the server requires session affinity (e.g. `https://hf.co/mcp`).

Each `tools.local[]` entry:

| Field | Type | Required |
|-------|------|----------|
| `name` | string | yes |
| `description` | string | yes — exposed to native tool APIs |
| `toolPrompt` | string | no — appended to description for LLM guidance |
| `inputSchema` | object | yes — JSON Schema for tool arguments |
| `runtime` | string | yes — `deno` |
| `script` | string | one of script/path |
| `path` | string | one of script/path (resolved relative to YAML at load) |
| `permissions` | string[] | no — `network:host` if `host.fetch()` used; no `network:*` |
| `timeoutMs` | integer | no |

Local tool scripts MUST define `function execute(arguments, host)` returning JSON-serializable data. When `path` ends in `.ts`, the reference runtime transpiles to JavaScript before sandbox execution (same approach as Deno guardrails).

**Tool discovery:** Providers with native function calling (OpenAI, Azure OpenAI, Anthropic, Grok) receive merged MCP + local tool schemas via the provider API. Providers without native tools (Gemini, Ollama) MUST receive an auto-generated tool catalog appended to the system prompt by conforming runtimes.

---

## 18. Semantic cache (`cache`)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | boolean | `false` | Enable caching for this endpoint |
| `mode` | string | `exact` | `exact` or `semantic` |
| `similarityThreshold` | number | `0.92` (semantic) | Minimum cosine similarity for semantic hit |
| `ttlSeconds` | integer | optional | Entry TTL |
| `maxEntries` | integer | optional | LRU cap per endpoint |
| `scope` | string | optional | Cache namespace (e.g. tenant id) |
| `excludeFields` | string[] | optional | Input fields omitted from cache key |
| `bypassOnGuardrailBlock` | boolean | `false` | Do not cache when guardrails block |
| `embedder.provider` | string | `hash` | `hash` (offline) or `openai` |
| `embedder.model` | string | optional | Embedding model when provider is `openai` |
| `store.type` | string | `memory` | `memory` or `redb` |
| `store.path` | string | optional | Path when `store.type` is `redb` |

Global env: `AIREST_CACHE_ENABLED`, `AIREST_CACHE_MAX_ENTRIES`, `AIREST_CACHE_STORE_PATH`, `AIREST_CACHE_EMBED_*`.

Sensitive, non-deterministic, or tool-enabled endpoints SHOULD set `enabled: false` explicitly. Use `preCacheWrite` guardrails to block caching regulated outputs.

---

## 19. Telemetry (`telemetry`)

| Field | Type | Default |
|-------|------|---------|
| `enabled` | boolean | `true` when block present |

When `enabled: false`, the runtime MUST NOT export OTEL traces/metrics for that endpoint. Use on PHI/PCI or privileged endpoints where trace export is a compliance risk.

---

## 20. Extended authentication (`auth.type`) — summary

Normative detail is in §6. Supported `auth.type` values: `apiKey`, `jwt`, `oauth2Introspect`, `trustGateway`, `none`.

---

## 21. Validation rules (normative summary)

Runtimes MUST reject definitions that violate any of the following at load time:

| Rule | Constraint |
|------|------------|
| R1 | `name` non-empty |
| R2 | `version` non-empty |
| R3 | `method` is `GET` or `POST` |
| R4 | `path` non-empty, starts with `/` |
| R5 | `systemPrompt` non-empty |
| R6 | `inputSchema` is object with `type: object` |
| R7 | `outputSchema` is object with `type: object` |
| R8 | `model.provider` is a supported provider value |
| R9 | `model.model` non-empty |
| R10 | `errors.*.message` non-empty when block present |
| R11 | `errors.*.status` in 400–599 when set |
| R12 | `health.status` in 200–599 when set |
| R13 | `health.message` non-empty when set |
| R14 | Unique `name` and `path` within a loaded set |
| R15 | When `tools.mcpServers` or `tools.local` is non-empty, `tools.allow` MUST be non-empty |
| R16 | Each `tools.allow` entry MUST contain `/` (qualified `server/tool` or `local/tool`) |
| R17 | Each `tools.local[]` entry MUST have `name`, `description`, `inputSchema`, and `script` or `path` |
| R18 | Hook `permissions` MUST NOT include `network:*` |
| R19 | Deno hooks using `fetch()` MUST declare `network:` permissions |
| R20 | Local tool scripts MUST NOT use `network:*`; fetch requires explicit `network:host` |

Runtimes SHOULD provide a validation command that checks definitions without starting the server.

---

## 22. Reference example

Full definition (from the aiREST reference examples):

```yaml
name: contract-risk-analyzer
category: legal
version: 1.0.0
description: Analyze contract text and return structured risk findings.

method: POST
path: /v1/analyze-contract-risk

auth:
  required: true

health:
  message: Contract risk analyzer is loaded and ready.
  status: 200

inputSchema:
  type: object
  additionalProperties: false
  required:
    - contractText
    - jurisdiction
  properties:
    contractText:
      type: string
      minLength: 50
    jurisdiction:
      type: string
      minLength: 2
    riskTolerance:
      type: string
      enum: [low, medium, high]

systemPrompt: |
  You are a senior legal contract analyst.
  Analyze the provided contract text for legal, financial, operational, and compliance risks.
  Return only valid JSON matching the required output schema.

userPromptTemplate: |
  Analyze the following contract.

  Contract Text:
  {{contractText}}

  Jurisdiction:
  {{jurisdiction}}

  Risk Tolerance:
  {{riskTolerance}}

outputSchema:
  type: object
  additionalProperties: false
  required:
    - summary
    - overallRisk
    - risks
    - missingClauses
    - recommendedActions
  properties:
    summary:
      type: string
    overallRisk:
      type: string
      enum: [low, medium, high]
    risks:
      type: array
      items:
        type: object
        required: [category, severity, description, recommendation]
        properties:
          category:
            type: string
          severity:
            type: string
            enum: [low, medium, high]
          description:
            type: string
          recommendation:
            type: string
    missingClauses:
      type: array
      items:
        type: string
    recommendedActions:
      type: array
      items:
        type: string

model:
  provider: openai
  model: gpt-4.1-mini
  temperature: 0.2
  maxTokens: 2000

policies:
  validateInput: true
  validateOutput: true
  retryOnInvalidJson: true
  retryOnInvalidSchema: true
  maxRetries: 2
  stripMarkdownCodeFences: true
  logRequests: true
  logResponses: false

errors:
  inputValidation:
    message: Contract request is missing required fields or contains invalid data.
  authentication:
    message: A valid x-api-key header is required for contract analysis.

examples:
  request:
    contractText: This agreement is entered into between Company A and Company B and includes payment terms, termination clauses, and confidentiality obligations sufficient for review.
    jurisdiction: Oklahoma
    riskTolerance: medium
  response:
    summary: The contract contains several moderate business and legal risks.
    overallRisk: medium
    risks: []
    missingClauses: []
    recommendedActions: []
```

---

## 23. Schema revision history

| Schema version | Date | Changes |
|----------------|------|---------|
| **1.0.0** | 2026-06-04 | Initial public draft. YAML definitions, JSON Schema Draft 7 input/output, multi-provider `model`, policies, auth, health, errors, examples. `GET` and `POST` methods (`GET` uses query parameters for input). |
| **1.0.0** (extensions) | 2026-06-04 | Reference implementation adds §15–§20: guardrails, hooks, MCP tools (stdio/HTTP/SSE), **local Deno tools**, semantic cache, telemetry, extended auth — backward-compatible optional fields. |
| **1.0.0** (extensions) | 2026-06-05 | Reference runtime: TypeScript transpilation for Deno guardrails and local tools; MCP HTTP `${ENV_VAR}` header expansion and session-id handling. |

---

## 25. Request execution pipeline (reference)

For a typical `POST` endpoint, the reference runtime executes hooks and guardrails in this order:

| Step | Component |
|------|-----------|
| 1 | Authenticate (`auth`) |
| 2 | `hooks.preRequest` |
| 3 | `guardrails` @ `preInput` |
| 4 | Validate `inputSchema` |
| 5 | `guardrails` @ `postInput`, `hooks.postInput` |
| 6 | Cache lookup (if `cache.enabled`) |
| 7 | `guardrails` @ `preLlm`, `hooks.preLlm` |
| 8 | Render prompts |
| 9 | LLM call **or** tool loop (`tools` — MCP + local, up to `maxToolRounds`) |
| 10 | `guardrails` @ `postLlm` |
| 11 | Parse JSON, validate `outputSchema` (retries per `policies`) |
| 12 | `guardrails` @ `postOutput`, `hooks.postOutput` |
| 13 | `guardrails` @ `preCacheWrite`, cache store |
| 14 | Success response (§12.1) |

Tool loop semantics (§17): native providers receive tool schemas via the provider API; Gemini and Ollama receive an auto-generated tool catalog in the system prompt.

---

## 24. Conformance

An implementation is **aiREST Definition Schema 1.0.0 conformant** if it:

1. Loads and validates definitions per §21  
2. Exposes HTTP routes per §5 for each loaded definition  
3. Validates inputs and outputs per §7 when policies require it  
4. Returns success and error envelopes per §12  
5. Honors `policies` retry semantics per §10  
6. Supports all `model.provider` values in §9.2 or documents a subset explicitly  

Implementations claiming **full reference parity** SHOULD also support §15–§20 (guardrails, hooks, tools, cache, telemetry, extended auth) as implemented in the [aiREST](https://github.com/gustaveherbst/airest) project.

Conformance tests and a formal JSON Schema meta-schema for definition files may be published in a future revision of this document.

---

## Related documents

- [README.md](./README.md) — project overview and quick start  
- [GOVERNANCE.md](./GOVERNANCE.md) — standard governance, n−1 versioning, deprecation  
- [examples/mcp/README.md](./examples/mcp/README.md) — MCP + local tool examples  
- [examples/tools/README.md](./examples/tools/README.md) — local Deno tool scripts  
- [docs/deploy/production.md](./docs/deploy/production.md) — production hardening  
- [examples/](./examples/) — reference definitions  
