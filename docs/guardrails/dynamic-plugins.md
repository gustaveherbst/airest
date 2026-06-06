# Dynamic guardrail plugins (.so / WASM)

**Status: deferred post-1.0**

aiREST ships two extension mechanisms today:

| Runtime | Use case |
|---------|----------|
| `builtin` | Fast, audited modules (`pii-redact`, `topic-allowlist`, …) |
| `deno` | Custom TypeScript via `deno_core` sandbox (`script` or `path`); `.ts` files are transpiled to JavaScript (SWC) before execution |

## Why deferred

Native `.so` and WASM plugins require:

- Stable ABI across Rust/toolchain versions
- Signing and load-time verification
- Memory/syscall isolation beyond the current Deno sandbox
- Hot-reload semantics for shared libraries

The roadmap keeps this as **TBD** until enterprise demand justifies the operational cost. Deno guardrails cover most custom logic without redeploying the binary.

## YAML today

Unknown `guardrails[].runtime` values are rejected at validate time (only `builtin` and `deno`).

```yaml
guardrails:
  - module: my-policy
    runtime: deno
    hook: preLlm
    path: ./guardrails/my-policy.ts
```

## Future sketch (not implemented)

1. `runtime: wasm` — Wasmtime module with WASI network disabled by default
2. `runtime: native` — explicit `.so` path + signature manifest loaded at startup
3. Registry API — `airest guardrails register ./plugin.wasm`

Track design changes via the governance change-proposal process in [GOVERNANCE.md](../../GOVERNANCE.md).
