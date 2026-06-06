# Contributing to aiREST

Thank you for your interest in aiREST. This document describes how to contribute to the reference runtime, examples, and standard documents.

## Ways to contribute

| Area | Examples |
|------|----------|
| **Runtime** | Bug fixes, performance, new providers, guardrails, MCP transports, cache, observability |
| **Examples** | New endpoint YAML under `examples/`, guardrail scripts, MCP/local-tool demos |
| **Documentation** | README, SCHEMA.md clarifications, deploy guides, example READMEs |
| **Tests** | Unit/integration tests, conformance fixtures |
| **Standard** | Schema or HTTP contract changes via Change Proposal (see below) |

Open a [GitHub issue](https://github.com/gustaveherbst/airest/issues) to discuss larger changes before investing significant effort.

## Development setup

```bash
git clone https://github.com/gustaveherbst/airest.git
cd airest
cp .env.example .env          # set OPENAI_API_KEY for live LLM tests
cargo build
cargo test
airest validate --folder ./examples
```

Optional smoke test of all bundled examples:

```bash
./examples/runall.sh
```

## Pull request checklist

1. **Scope** — One logical change per PR when possible.
2. **Tests** — Add or update tests for behavior changes; `cargo test` must pass.
3. **Examples** — If you change validation or runtime behavior, update affected YAML under `examples/` and run `airest validate --folder ./examples`.
4. **Docs** — Update README, SCHEMA.md, or example READMEs when user-visible behavior changes.
5. **Style** — Match surrounding Rust and YAML conventions; avoid unrelated formatting churn.

## Normative changes (SCHEMA.md / HTTP contract)

Changes to required fields, validation rules, error envelopes, or API paths are **standard changes**, not ordinary implementation PRs. Follow the Change Proposal process in **[GOVERNANCE.md §7](GOVERNANCE.md#7-change-process)**:

1. Open an issue or draft PR describing the **problem**, **proposal**, **version impact**, **migration**, and **conformance tests**.
2. Wait for maintainer feedback before merging normative document edits.
3. Implementation PRs that depend on a spec change should reference the accepted CP.

Editorial fixes (typos, examples) can go through a normal PR.

## Implementation-only changes

Internal refactors, optimizations, and new **optional** YAML fields that are already accepted via CP may use the normal PR flow. Defer experimental capabilities documented as post-1.0 (e.g. [docs/guardrails/dynamic-plugins.md](docs/guardrails/dynamic-plugins.md)) until promoted through governance.

## Recognition

Contributors are credited through git history and GitHub pull request authorship. Significant ongoing maintainers are listed in repository settings and release notes.

## Questions

- **Schema / API semantics** — [SCHEMA.md](SCHEMA.md) and [GOVERNANCE.md](GOVERNANCE.md)
- **Production deployment** — [docs/deploy/production.md](docs/deploy/production.md)
- **Examples** — [examples/README.md](examples/README.md)
