# Local tool scripts

TypeScript modules referenced from endpoint YAML via `tools.local[].path`. aiREST transpiles `.ts` files to JavaScript (SWC) before executing them in the Deno sandbox.

Each module must define:

```javascript
function execute(arguments, host) {
  // arguments: tool input from the model
  // host.requestId — current request id
  // host.fetch(url) — only when permissions include network:host
  return { /* JSON-serializable result */ };
}
```

See `examples/mcp/kb-ticket-search-local.yaml` and [`../mcp/README.md`](../mcp/README.md).
