## Summary

<!-- What changed and why? One or two sentences focused on intent, not a file list. -->

## Type of change

- [ ] Bug fix (non-breaking runtime or validation fix)
- [ ] Feature (new optional capability or provider integration)
- [ ] Documentation (README, SCHEMA.md, examples, deploy guides)
- [ ] Examples only (`examples/` YAML, guardrails, MCP demos)
- [ ] Tests / CI
- [ ] Normative standard change (`SCHEMA.md`, HTTP contract, or `GOVERNANCE.md`)

## Related issues

<!-- Link issues or Change Proposals, e.g. Fixes #123 or CP #456 -->

## Normative changes (if applicable)

Skip this section for implementation-only PRs.

- [ ] This PR does **not** change normative schema or HTTP contract behavior
- [ ] Change Proposal accepted or opened: <!-- link -->
- [ ] **Version impact:** schema MAJOR / MINOR / PATCH — API major if any
- [ ] **Migration:** how existing definitions and clients are affected
- [ ] Conformance tests or fixtures updated

See [GOVERNANCE.md §7](../GOVERNANCE.md#7-change-process).

## Checklist

- [ ] `cargo test` passes locally
- [ ] `cargo test --features otel` passes (if OTEL code touched)
- [ ] `airest validate --folder ./examples` passes (if loader, validator, or examples changed)
- [ ] Tests added or updated for behavior changes
- [ ] Docs updated for user-visible changes (README, SCHEMA.md, example READMEs)
- [ ] Scope is focused — no unrelated refactors or formatting churn

## Test plan

<!-- How did you verify this? Commands run, endpoints exercised, before/after behavior. -->

```bash
# e.g.
cargo test
airest validate --folder ./examples
```
