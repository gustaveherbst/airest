# aiREST Standard Governance

| | |
|---|---|
| **Document version** | `1.0.0` |
| **Status** | Draft |
| **Date** | 2026-06-04 |
| **Applies to** | Definition Schema, HTTP API contract, reference runtime |
| **Normative schema** | [SCHEMA.md](./SCHEMA.md) |
| **Current schema version** | `1.0.0` |

This document defines how the **aiREST standard** is governed: who decides changes, how versions are numbered, how breaking changes are introduced, and how **current plus n−1** versions of the **Definition Schema** and **HTTP API contract** MUST be supported by conformant runtimes and SHOULD be supported by ecosystem tooling.

Keywords **MUST**, **SHOULD**, **MAY**, **MUST NOT**, and **SHOULD NOT** are used in the RFC 2119 sense.

---

## 1. Purpose

aiREST standardizes declarative, contract-first AI REST APIs. Governance exists to:

1. Keep the standard **predictable** for authors, operators, and client integrators
2. Allow **evolution** without silently breaking deployed definitions or clients
3. Require **backward compatibility windows** so adopters can migrate on a known schedule
4. Separate **specification authority** (schema + HTTP contract) from **implementation velocity** (reference runtime features)

The standard comprises three versioned surfaces:

| Surface | What it versions | Where it lives |
|---------|------------------|----------------|
| **Definition Schema** | YAML definition file format | [SCHEMA.md](./SCHEMA.md) |
| **HTTP API contract** | Request/response envelopes, error types, headers | SCHEMA.md §12; this document §5 |
| **Endpoint definition** | Semantics of one AI capability | `version` field in each YAML file |

The reference Rust runtime ([aiREST](https://github.com/gustaveherbst/airest)) implements the standard but does not replace it. Other runtimes MAY claim conformance independently.

---

## 2. Roles and authority

### 2.1 Standard maintainers

Maintainers hold merge authority over normative documents (`SCHEMA.md`, `GOVERNANCE.md`, conformance fixtures) and release tags that mark schema/API versions.

Responsibilities:

- Triage change proposals
- Publish schema and API version bumps
- Maintain the **support matrix** (§6)
- Run or delegate conformance review for implementations claiming aiREST compliance

Until a formal steering group exists, maintainers are the project owners listed in the repository. A future **aiREST Technical Steering Committee (TSC)** MAY supersede this section via an amendment to this document.

### 2.2 Contributors

Anyone MAY open issues, pull requests, or **Change Proposals** (§7). Contributors do not unilaterally change normative behavior on `main` without maintainer review.

### 2.3 Implementers

Vendors and teams shipping aiREST-compatible runtimes MUST document which schema and API versions they support and MUST honor the n−1 policy (§4) when claiming full conformance for a given version.

### 2.4 Definition authors

Teams authoring YAML definitions own their endpoint `version` field and migration of their `path` prefixes. They MUST target a supported Definition Schema version and SHOULD declare intent when upgrading (§8).

---

## 3. Version numbering

All standard versions use **Semantic Versioning 2.0.0** (`MAJOR.MINOR.PATCH`).

### 3.1 Definition Schema version

The schema version identifies the normative YAML format (e.g. `1.0.0` in SCHEMA.md header).

| Bump | When |
|------|------|
| **MAJOR** | Breaking change to required fields, field semantics, validation rules, or HTTP contract that invalidates existing conformant definitions or clients |
| **MINOR** | Backward-compatible additions (new optional fields, new providers, new policy keys) |
| **PATCH** | Clarifications, typo fixes, non-normative examples; no behavior change |

When schema **1.1.0** ships, runtimes that claim **1.1.0** conformance MUST still accept **1.0.x** definitions (n−1 major support applies only across major bumps; see §4).

### 3.2 HTTP API contract version

The API contract version is carried in URL paths and response metadata:

- Path prefix: `/v1/...`, `/v2/...`
- Future optional header: `Accept-Version: 1` (reserved; not required in 1.0.0)

| Bump | When |
|------|------|
| **MAJOR** (path `/vN`) | Breaking change to success/error envelope shape, required meta fields, or standard error type semantics |
| **MINOR** | Additive response fields, new optional headers |
| **PATCH** | Documentation-only |

Each endpoint definition’s `path` MUST include exactly one API major prefix (e.g. `/v1/sentiment-analyzer`). Authors introduce `/v2/...` by publishing a **new definition** (new file or new `name`/`path`), not by silently rewriting `/v1/...`.

### 3.3 Endpoint definition version

The YAML `version` field (e.g. `1.2.0`) versions **one endpoint’s contract** — input/output schemas, prompts, and behavior — independent of the Definition Schema version.

Rules:

- **PATCH**: prompt tuning, description, non-breaking output additions with compatible clients
- **MINOR**: backward-compatible `inputSchema` / `outputSchema` extensions (new optional properties)
- **MAJOR**: breaking input/output changes; SHOULD be paired with a new path prefix (`/v2/...`) or a new `name`

The runtime returns this value in `meta.version` on every response (SCHEMA.md §12).

---

## 4. n−1 support policy

Conformant runtimes MUST support **the current standard version and the immediately previous major version** for both the Definition Schema and the HTTP API contract. This is the **n−1 window**.

### 4.1 Definition Schema n−1

When the current schema is **N.M.P**, runtimes MUST:

| Requirement | Detail |
|-------------|--------|
| **Accept** | Definitions valid under current schema **N** and previous major **N−1** |
| **Validate** | Apply the ruleset matching each definition’s declared schema (see §4.3) |
| **Reject gracefully** | Definitions declaring schema **N−2** or older: clear error at load time, not at request time |
| **Document** | Publish supported schema majors in release notes and runtime `--version` / health output |

Example: when schema **2.0.0** is current, runtimes MUST load and serve **2.x** and **1.x** definitions. **0.x** (if ever published) is out of support unless explicitly extended by maintainer LTS policy.

### 4.2 HTTP API contract n−1

When the current API contract major is **vK**, runtimes MUST:

| Requirement | Detail |
|-------------|--------|
| **Serve** | Routes registered under `/vK/` and `/v(K−1)/` for loaded definitions |
| **Envelope** | Return response shapes per the API major implied by the request path |
| **Errors** | Map failures to the error envelope of the API major being invoked |
| **Meta** | Include `meta.version` (endpoint definition version) in all success and error responses |

Example: when **v3** is current, `/v3/...` and `/v2/...` MUST both work for definitions that target those paths. `/v1/...` MAY be removed after the deprecation window (§5) unless covered by an LTS exception.

### 4.3 Declaring schema version in definitions

Schema **1.0.0** does not yet require an in-file schema declaration. Starting with schema **1.1.0**, definitions SHOULD include:

```yaml
schemaVersion: "1.1.0"
```

Rules once `schemaVersion` exists:

- Omitted `schemaVersion` in files authored before 1.1.0 MUST be interpreted as **1.0.0**
- Runtimes MUST NOT guess; they MUST use the declared value or the default above
- Mismatch between file content and declared `schemaVersion` is an `ENDPOINT_DEFINITION_ERROR`

### 4.4 Overlap period

When a new **major** schema or API version is published, maintainers MUST:

1. Announce the release date and n−1 end date (§5)
2. Ship reference runtime support for **both** majors before dropping the older major
3. Keep SCHEMA.md revision history and the support matrix (§6) updated

Minimum overlap: **6 months** from GA of the new major until the previous major MAY be removed from conformant runtimes. Maintainers MAY extend overlap for enterprise LTS tracks.

---

## 5. Deprecation and removal

### 5.1 Deprecation stages

| Stage | Meaning |
|-------|---------|
| **Active** | Fully supported; recommended for new work |
| **Deprecated** | Still supported per n−1; SHOULD NOT be used for new definitions |
| **Sunset** | Removed from conformant runtimes; existing deployments MUST migrate |

### 5.2 Definition Schema majors

- A schema major moves to **Deprecated** when the next major reaches **Active**
- It moves to **Sunset** no earlier than **6 months** after deprecation, following public notice
- Sunset requires a **major** bump of the reference runtime’s conformance claim

### 5.3 HTTP API majors

- `/v(K−1)/` follows the same Deprecated → Sunset timeline as schema **K−1** when the API major and schema major are released together
- If only the API major changes, path sunset is independent but MUST still respect the n−1 minimum overlap

### 5.4 Endpoint definitions

Authors MAY deprecate an endpoint by:

1. Documenting deprecation in `description`
2. Serving a successor at a new path (e.g. `/v2/foo` replacing `/v1/foo`)
3. Returning consistent `meta.version` so clients can detect upgrades

The standard does not require a machine-readable deprecation field in 1.0.0; a future optional `lifecycle.status` field MAY be added in a minor schema release.

### 5.5 Communication

Deprecations MUST be recorded in:

- SCHEMA.md revision history
- GOVERNANCE.md support matrix (§6)
- Project changelog or release notes
- Optional: `Deprecation:` HTTP header on responses (implementation-defined until standardized)

---

## 6. Support matrix

Maintainers MUST keep this table accurate at each schema/API release.

| Definition Schema | HTTP API | Status | Reference runtime | Notes |
|-------------------|----------|--------|-------------------|-------|
| **1.0.0** | **v1** | Active | ≥ 0.1.0 | Initial public draft |

**Legend:** *Active* = current recommended; *Deprecated* = n−1, supported but not for new designs; *Sunset* = not required for conformance.

When **2.0.0** / **v2** ship, the matrix MUST show **1.0.x** / **v1** as Deprecated until sunset.

---

## 7. Change process

### 7.1 Change classes

| Class | Examples | Process |
|-------|----------|---------|
| **Editorial** | Typos, examples | PR to normative doc; PATCH doc version |
| **Minor spec** | New optional YAML field, new provider | Change Proposal (CP) + review; MINOR schema bump |
| **Major spec** | Required field rename, envelope break | CP + public comment period (≥ 14 days) + MAJOR bump |
| **Implementation-only** | Internal runtime optimization | Normal PR; no schema bump |

### 7.2 Change Proposal (CP)

Non-editorial spec changes SHOULD start as a CP (GitHub issue or pull request) containing:

1. **Problem** — what gap or pain exists
2. **Proposal** — exact field/API changes with YAML and HTTP examples
3. **Version impact** — schema MAJOR/MINOR/PATCH and API major if any
4. **Migration** — how n−1 authors and clients upgrade
5. **Conformance tests** — new or updated fixtures

Maintainers accept or reject CPs. Accepted CPs become PRs against SCHEMA.md and, when needed, GOVERNANCE.md.

### 7.3 Release tagging

| Artifact | Tag pattern | Example |
|----------|-------------|---------|
| Schema document | `schema-vMAJOR.MINOR.PATCH` | `schema-v1.0.0` |
| Reference runtime | `vMAJOR.MINOR.PATCH` | `v0.1.0` |

Schema tags mark normative document snapshots. Runtime tags mark implementation releases. A runtime release SHOULD cite the highest schema version it fully implements.

---

## 8. Migration guidance

### 8.1 Upgrading Definition Schema (author)

1. Identify target schema version in SCHEMA.md revision history
2. Add `schemaVersion` when available
3. Run validation tooling (`airest validate`) against the target runtime
4. Fix breaking deltas listed in the major-version migration notes
5. Bump endpoint `version` per semver for your contract changes
6. Deploy alongside existing definitions during the n−1 overlap; do not delete old-major files until clients migrate

### 8.2 Upgrading HTTP API (author)

1. Copy definition to a new path with the new prefix (`/v2/...`)
2. Bump endpoint `version` if input/output contracts change
3. Run old and new routes in parallel during deprecation window
4. Update client base URLs and response parsing for new envelope fields if API major changed

### 8.3 Upgrading clients

Clients SHOULD:

- Pin integration tests to a specific API major path (`/v1/...` or `/v2/...`)
- Read `meta.version` for endpoint-level semver
- Tolerate additive fields in success and error envelopes within the same API major
- Plan migration when `/v(K−1)/` enters Sunset per maintainer announcements

---

## 9. Conformance and compatibility claims

### 9.1 Levels

| Claim | Meaning |
|-------|---------|
| **Schema X.Y.Z conformant** | Loads and validates definitions per SCHEMA.md for major X, plus n−1 |
| **API vK conformant** | HTTP behavior matches SCHEMA.md §12 for major K, plus v(K−1) |
| **Full aiREST conformant** | Both schema and API claims for stated versions, including n−1 |

Implementers MUST NOT claim a version they do not fully support. Subset implementations (e.g. OpenAI-only providers) MUST document omissions.

### 9.2 Conformance tests

Maintainers SHOULD publish a versioned test corpus:

```text
conformance/
  schema-1.0/
  schema-2.0/
  api-v1/
  api-v2/
```

Runtimes SHOULD CI against the corpus for every version they claim. Formal certification is out of scope for 1.0.0 governance but MAY be added later.

### 9.3 Reference implementation

The aiREST Rust runtime is the **reference implementation**. When spec and reference diverge, **SCHEMA.md wins** until a spec bug is confirmed and patched. Reference runtime fixes that restore spec compliance do not require a schema bump.

---

## 10. Security and stability

- **Security fixes** that change validation or auth behavior MAY ship as PATCH schema/runtime releases; if behavior visible to clients changes, release notes MUST call out migration steps
- **Secret handling** is runtime configuration (environment variables), not part of the YAML schema; governance does not standardize provider API keys
- **Experimental features** documented as deferred (e.g. [docs/guardrails/dynamic-plugins.md](./docs/guardrails/dynamic-plugins.md)) MUST NOT alter normative 1.0.0 fields until promoted through the CP process

---

## 11. Amendments to this document

Changes to GOVERNANCE.md follow the same CP process as major/minor spec changes. Material changes to n−1 policy or roles require:

1. Public CP with rationale
2. Maintainer approval
3. **MINOR** bump of this document’s version for additive/clarifying changes; **MAJOR** bump if support windows or voting rules change

---

## 12. Related documents

| Document | Role |
|----------|------|
| [SCHEMA.md](./SCHEMA.md) | Normative Definition Schema and HTTP response contract |
| [README.md](./README.md) | Project overview and quick start |
| [examples/](./examples/) | Illustrative definitions (non-normative) |
| [docs/guardrails/dynamic-plugins.md](./docs/guardrails/dynamic-plugins.md) | Deferred native/WASM guardrail plugins |

---

## Revision history

| Document version | Date | Changes |
|------------------|------|---------|
| **1.0.0** | 2026-06-04 | Initial governance: roles, semver surfaces, n−1 schema and API policy, deprecation timeline, CP process, support matrix |
