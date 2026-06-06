# MCP and local tool examples

aiREST connects to MCP servers and **local Deno tools** internally — callers use the REST route only.

## Public Hugging Face MCP (no local mock)

**`kb-ticket-search-hf.yaml`** calls the public server at **`https://hf.co/mcp`**. No mock process required.

| File | Route | MCP server |
|------|-------|------------|
| `kb-ticket-search-hf.yaml` | `POST /v1/search-support-kb` | `https://hf.co/mcp` (`huggingface/hf_doc_search`) |

```yaml
tools:
  mcpServers:
    - name: huggingface
      transport: http
      url: https://hf.co/mcp
      headers:
        Authorization: "Bearer ${HF_TOKEN}"
  allow:
    - huggingface/hf_doc_search
```

Optional: set `HF_TOKEN` in your environment for higher rate limits. If unset, the `Authorization` header is omitted and anonymous access is used.

```bash
airest serve --folder ./examples/mcp --no-recursive
airest test kb-ticket-search-hf --folder ./examples/mcp
```

```bash
curl -X POST http://localhost:3300/v1/search-support-kb \
  -H "Content-Type: application/json" \
  -H "x-api-key: $AIREST_API_KEY" \
  -d '{"query": "how do I authenticate with the Hugging Face Hub API?"}'
```

Expect a JSON response with `summary` and `matches[]` doc links when the LLM invokes `hf_doc_search` (typically ~5s latency).

## MCP transports

| Transport | YAML value | When to use |
|-----------|------------|-------------|
| **stdio** | `stdio` | Local subprocess (dev, sidecar in same pod) |
| **HTTP** | `http` or `streamableHttp` | Remote MCP server with JSON-RPC POST |
| **SSE** | `sse` | Remote MCP server with SSE session + message POST |

## All MCP / tool files in this folder

| File | Route | Transport |
|------|-------|-----------|
| `kb-ticket-search-hf.yaml` | `POST /v1/search-support-kb` | `http` — **public HF MCP** |
| `kb-ticket-search-http.yaml` | `POST /v1/search-support-kb-http` | `http` — local mock |
| `kb-ticket-search-sse.yaml` | `POST /v1/search-support-kb-sse` | `sse` — local mock |
| `kb-ticket-search-local.yaml` | `POST /v1/search-support-kb-local` | local Deno tool (no MCP) |

Every tool-enabled endpoint **must** declare `tools.allow` (qualified `serverName/tool_name` or `local/tool_name`).

### How the LLM learns about tools

- **OpenAI / Anthropic / Azure / Grok:** tool schemas are sent via native function-calling APIs (no prompt injection required).
- **Gemini / Ollama:** aiREST auto-appends a tool catalog to the system prompt and accepts JSON tool-invocation responses.

## Local tools (no MCP server)

TypeScript tools run in the Deno sandbox (same security model as hooks). The reference runtime **transpiles `.ts` to JavaScript** (SWC) before execution. Qualified allow entry: `local/<toolName>`.

```yaml
tools:
  local:
    - name: search_kb
      description: Search the support KB
      toolPrompt: Call when historical ticket context is needed.
      inputSchema: { type: object, required: [query], properties: { query: { type: string } } }
      runtime: deno
      path: ../tools/search-kb-local.ts
      permissions: []
  allow:
    - local/search_kb
```

Script contract: `function execute(arguments, host) { return { ... }; }`

## Mock HTTP/SSE server (http + sse examples only)

The **`kb-ticket-search-http.yaml`** and **`kb-ticket-search-sse.yaml`** examples use a local mock — not required for the HF example above.

Start the HTTP+SSE mock (default port **3100**):

```bash
node examples/mcp/mcp-mock-kb-remote.mjs
# or: MCP_PORT=3200 node examples/mcp/mcp-mock-kb-remote.mjs
```

```bash
# Terminal 1 — mock MCP server
node examples/mcp/mcp-mock-kb-remote.mjs

# Terminal 2 — aiREST (mock HTTP/SSE examples only; omit HF yaml or use --no-recursive with selected files)
airest serve --folder ./examples/mcp --no-recursive
airest validate --folder ./examples/mcp
```

Mock server config pattern — optional `headers` with `${ENV_VAR}` placeholders:

```yaml
tools:
  mcpServers:
    - name: support-kb
      transport: http
      url: http://127.0.0.1:3100/mcp
      headers:
        Authorization: "Bearer ${MCP_API_TOKEN}"
```

`${ENV_VAR}` placeholders expand from the process environment; headers whose placeholders are unset are omitted.

Local stdio mock (`../support/mcp-mock-kb.mjs`) remains in `support/` for unit tests only.

See also [`docs/deploy/production.md`](../../docs/deploy/production.md) (MCP allowlists).
