# Security Policy

## Supported versions

Security fixes are applied to the **latest release** on the default branch (`main`) and, when practical, backported to the most recent tagged release. Older tags are not routinely maintained unless announced in release notes.

| Version | Supported |
|---------|-----------|
| Latest `main` | Yes |
| Latest tagged release (`v*`) | Yes |
| Older tagged releases | Best effort |

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Report them privately using one of these channels:

1. **[GitHub Security Advisories](https://github.com/gustaveherbst/airest/security/advisories/new)** (preferred) — use *Report a vulnerability* on the repository Security tab.
2. **Repository owner** — contact details on the [project homepage](https://github.com/gustaveherbst/airest).

Include as much detail as you can:

- Description of the issue and potential impact
- Steps to reproduce or a proof of concept
- Affected versions or commits
- Suggested fix, if you have one

We aim to acknowledge reports within **5 business days** and will keep you informed of progress. We may ask for additional information before publishing a fix.

## Disclosure

When a fix is available, we will:

1. Patch `main` and prepare a patch or minor release as appropriate
2. Credit reporters in release notes when they agree to be named
3. Publish a GitHub Security Advisory when the issue warrants public tracking

Please allow reasonable time for remediation before public disclosure. We follow coordinated disclosure in good faith.

## Scope

In scope for this policy:

- The aiREST reference runtime (Rust server, CLI, loaders, auth, guardrails, hooks, MCP client, cache)
- Normative validation behavior that could bypass auth, guardrails, or sandbox restrictions
- Supply-chain or CI/release pipeline issues affecting official binaries published from this repository

Generally **out of scope**:

- Misconfiguration of deployment environments (missing TLS, weak API keys left in client apps)
- Vulnerabilities in third-party LLM providers, MCP servers, or upstream dependencies already tracked by their maintainers — report those upstream, then notify us if aiREST-specific integration amplifies impact
- Denial-of-service from expected load without a defect in request limits or concurrency controls
- Issues in user-authored YAML definitions or Deno guardrail/hook scripts (authors are responsible for reviewing their own endpoint logic)

## Security model (reference runtime)

Operators and contributors should be familiar with how aiREST is designed to be deployed safely:

| Area | Behavior |
|------|----------|
| **Auth** | Endpoints may require API key, JWT, OAuth2 introspection, or trust-gateway headers; production mode rejects placeholder secrets |
| **Guardrails** | Built-in and Deno modules run at defined pipeline hooks; Deno scripts are sandboxed (no filesystem/network unless explicitly permitted in hooks) |
| **MCP / tools** | `tools.allow` is required; callers never invoke MCP directly |
| **Secrets** | Provider keys and API keys are environment variables, not YAML fields |
| **Production** | Body size limits, request timeouts, optional graceful shutdown, circuit breakers — see [docs/deploy/production.md](docs/deploy/production.md) |

If you find a way to bypass these controls, treat it as a high-priority report.

## Hardening recommendations

When running aiREST in production:

- Set `AIREST_PRODUCTION=true` and strong, non-placeholder secrets
- Terminate TLS at a gateway (Kong/Envoy); see [examples/gateway/README.md](examples/gateway/README.md)
- Declare guardrails on every sensitive endpoint; enable log redaction for PII
- Keep the runtime and dependencies updated via tagged releases
- Restrict network access to MCP servers and LLM providers as needed

## Questions

For non-sensitive security questions (configuration, best practices), open a [GitHub issue](https://github.com/gustaveherbst/airest/issues) with the **question** label or refer to [docs/deploy/production.md](docs/deploy/production.md).
