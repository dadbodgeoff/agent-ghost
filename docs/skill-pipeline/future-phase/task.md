# Skill Pipeline Future Phase Tasks

## Intent

This document is the execution plan for the external-skill future phase.
It is derived from:

- [future-phase.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/skill-pipeline/future-phase.md)

This file is intentionally task-shaped:

- ordered work
- concrete acceptance criteria
- explicit verification commands
- rollout checkpoints
- adversarial testing engineered to force real production bugs to surface
  before rollout

## Execution Rules

- Keep the current compiled-skill truth model intact until the replacement path
  is fully real.
- Do not execute skills directly from workspace roots, user directories, or
  ad hoc artifact paths.
- Do not treat `signature.is_some()` or any equivalent placeholder as
  verification.
- Do not let quarantine exist as UI copy without hard runtime blocking.
- Do not expose external-skill install or execute flows before the catalog,
  verification, and state model are authoritative.
- Do not start broad WASM execution work before artifact, ingestion, signing,
  and quarantine are complete.
- Default deny every new host capability until explicitly designed, tested, and
  documented.
- Every trust-boundary task must land with adversarial tests, not only happy
  path coverage.
- Every persisted lifecycle state must survive restart and be reloaded
  truthfully.
- Every operator override or quarantine resolution path must emit audit data.

## Current Checkpoint Notes

This task list remains the governing order, but the live repo has advanced past
the original baseline in these areas:

- T1-T4 foundations are in place: artifact format, deterministic digesting,
  signing hooks, persistent lifecycle state, and managed ingestion storage
- T5 is live for mixed-source catalog truth in `/api/skills` and install state
  projection
- T7 is live for operator quarantine, stale-revision-safe resolution, and
  managed-artifact reverification
- T9-T10 are live for the current contract: OpenAPI, generated SDK types, SDK
  client surfaces, CLI, and dashboard skill operator flows now reflect the
  mixed-source catalog state model
- T8 is live for mixed-source runtime alignment: runtime/tool exposure, policy
  grants, and direct execute resolve through the catalog-owned path for both
  compiled and external skills
- T11 is live in its current narrow form: external WASM executes through
  `wasmtime` from gateway-managed artifacts only, with timeout, fuel, memory
  limits, zero host imports, and deterministic API error mapping
- T12 is live for the initial adversarial sandbox suite: hidden-import probes,
  env/filesystem/network/process denial, tampered managed artifacts, sandbox
  quarantine, and timeout/fuel/memory exhaustion all have committed tests

Still outstanding in this plan:

- T13 final governance/doc truthfulness pass across all skill docs
- T14 full rollout evidence and package-wide verification closeout

## Adversarial Verification Standard

This phase requires top-tier adversarial verification aimed at rooting out true
production bugs rather than merely proving a nominal happy path.

That means:

- every parser, manifest, digest, and signature boundary gets invalid,
  malicious, and ambiguity-focused tests
- every ingestion boundary gets TOCTOU, symlink, duplicate-name, and path
  traversal tests
- every runtime exposure rule gets bypass-attempt tests
- every WASM host capability gets fail-closed tests
- every operator resolution flow gets stale-state and replay tests

A task is not complete if it only proves success cases.

## Exit Criteria

The future phase is only complete when all of these are true:

1. external skill artifacts have one canonical package format with
   deterministic hashing and schema validation
2. gateway ingestion persists authoritative artifact state and runtime never
   executes directly from source roots
3. verification, quarantine, install, disable, and runtime exposure are
   represented as separate enforced states
4. `/api/skills` truthfully lists compiled and external skills with source,
   signer, digest, verification status, quarantine state, and runtime
   availability
5. install, uninstall, enable, disable, and quarantine transitions change
   runtime-visible behavior deterministically
6. tampered, unsigned, untrusted, revoked, invalid, or quarantined skills
   cannot execute
7. runtime skill exposure, tool registration, and `skill:<name>` policy grants
   are aligned under mixed-source tests
8. external skill execution uses the catalog-owned execution seam rather than
   any direct runner bypass
9. untrusted WASM skills execute only through a restricted host ABI with
   timeout, fuel, and memory enforcement
10. dashboard, SDK, and CLI all show the same signer, privilege, verification,
    quarantine, and install truth
11. docs describe exactly what is live, what is gated, and what is still not
    supported
12. adversarial test suites cover the known bypass, tamper, sandbox, and
    persistence failure classes for this phase

## Task Sequence

### T0. Freeze ADRs and Phase Boundaries

Deliver:

- artifact format ADR
- signer trust model ADR
- quarantine state-machine ADR
- WASM host ABI ADR
- rollout gate ADR

Primary files:

- `docs/skill-pipeline/future-phase/`
- supporting ADR locations already used by the repo, if any

Acceptance criteria:

- one external artifact format is chosen
- one signer trust-root and revocation model is chosen
- one quarantine lifecycle is chosen
- one minimal WASM host ABI is chosen
- rollout stages explicitly forbid premature external execution

Verification:

- manual doc review against [future-phase.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/skill-pipeline/future-phase.md)
- grep review for contradictory external-skill claims in touched docs

### T1. Build the External Artifact and Manifest Model

Deliver:

- `SkillManifestV1`
- `SkillArtifact`
- `SkillSignatureEnvelope`
- canonical digesting and normalization logic
- typed validation errors

Primary files:

- `crates/ghost-skills/src/`
- expected new module area such as `crates/ghost-skills/src/artifact/`
- `crates/ghost-skills/src/lib.rs`

Acceptance criteria:

- manifest schema is versioned explicitly
- hashing is deterministic across machines and file ordering
- signatures bind normalized manifest plus content digests
- manifest validation fails closed on unknown schema versions or missing fields

Verification:

- add unit tests for normalization and schema validation
- add property tests for digest and signature stability
- `cargo test -p ghost-skills`

### T2. Build Packaging, Inspect, and Validation Tooling

Deliver:

- package creation command
- package inspect command
- package validate command
- reusable artifact fixtures for tests

Primary files:

- `crates/ghost-gateway/src/cli/skill.rs`
- `crates/ghost-skills/tests/`
- any new packaging helpers under `crates/ghost-skills/src/`

Acceptance criteria:

- the same source tree yields the same artifact digest on repeated builds
- inspect output shows version, publisher, signer, digest, privileges, and
  requested capabilities
- packaging rejects path traversal, duplicate logical paths, and malformed
  manifests

Verification:

- add CLI tests for package, inspect, and validate flows
- add adversarial tests for zip-slip and duplicate-path rejection
- `cargo test -p ghost-gateway`
- `cargo test -p ghost-skills`

### T3. Add Persistent Storage for Artifact, Verification, and Quarantine State

Deliver:

- migrations for external artifacts, versions, signer trust roots,
  verification results, quarantine decisions, and install state
- query helpers for read, upsert, transition, and audit views

Primary files:

- `crates/cortex/cortex-storage/src/migrations/`
- `crates/cortex/cortex-storage/src/queries/`

Acceptance criteria:

- database can represent artifact identity separately from lifecycle state
- verification records are tied to exact artifact digests
- quarantine records carry reason, actor, and timestamps
- install state is version-aware for external skills

Verification:

- add migration and query tests
- add restart-persistence tests for lifecycle reload
- `cargo test -p ghost-gateway`

### T4. Implement Gateway Ingestion and Managed Artifact Storage

Deliver:

- ingestion scanner for approved roots
- gateway-managed artifact copy/store
- source-to-artifact reconciliation logic
- ingestion audit events

Primary files:

- expected new module area such as `crates/ghost-gateway/src/skill_ingest/`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/state.rs`

Acceptance criteria:

- source roots are inputs only, not runtime authorities
- ingestion copies artifacts into gateway-managed storage before they are
  considered for verification or execution
- deleting or mutating source files after ingestion does not silently rewrite
  runtime truth
- symlink and path-traversal tricks are rejected

Verification:

- add integration tests for source deletion after ingest
- add adversarial tests for symlink swap and TOCTOU ingestion races
- `cargo test -p ghost-gateway`

Checkpoint:

- stop here and verify that external artifacts can be ingested and persisted
  without any runtime execution path being enabled

### T5. Generalize the Gateway Skill Catalog to Mixed Sources

Deliver:

- catalog support for compiled, user, and workspace skill sources
- catalog DTOs carrying source, signer, digest, verification, quarantine, and
  lifecycle state
- mixed-source resolution helpers for runtime and API use

Primary files:

- `crates/ghost-gateway/src/skill_catalog/mod.rs`
- `crates/ghost-gateway/src/skill_catalog/definitions.rs`
- `crates/ghost-gateway/src/skill_catalog/dto.rs`
- `crates/ghost-gateway/src/skill_catalog/service.rs`

Acceptance criteria:

- one catalog lists all skill sources truthfully
- compiled and external skills share the same runtime resolution model
- verification and quarantine state are visible without inventing semantics in
  clients
- non-runtime-visible skills do not appear executable

Verification:

- add unit tests for mixed-source catalog merging
- add API-shape tests for DTO truth
- `cargo test -p ghost-gateway`

### T6. Integrate Real Signing, Trust Roots, and Revocation

Deliver:

- real verification flow using `ghost-signing`
- configured trust roots for accepted publishers
- revocation handling
- deterministic verification results bound to artifact digests

Primary files:

- `crates/ghost-signing/src/lib.rs`
- `crates/ghost-signing/src/signer.rs`
- `crates/ghost-signing/src/verifier.rs`
- expected new verification modules in `crates/ghost-gateway/src/`
- `crates/ghost-skills/src/registry.rs`

Acceptance criteria:

- signature verification is cryptographic and digest-bound
- missing, invalid, unknown, or revoked signers fail verification
- verification cannot be replayed onto a different artifact digest
- signer identity is surfaced to the catalog and operator surfaces

Verification:

- extend signing unit and property tests
- add gateway integration tests for valid, invalid, unknown, and revoked
  signers
- `cargo test -p ghost-signing`
- `cargo test -p ghost-gateway`

### T7. Implement Quarantine State Machine and Operator Controls

Deliver:

- quarantine reason taxonomy
- quarantine transition rules
- operator resolution and re-verification flows
- audit emission for all quarantine transitions

Primary files:

- expected new quarantine modules in `crates/ghost-gateway/src/`
- `crates/ghost-gateway/src/api/skills.rs`
- `crates/ghost-gateway/src/cli/skill.rs`
- `crates/ghost-gateway/src/skill_catalog/service.rs`

Acceptance criteria:

- quarantine blocks install, enable, runtime exposure, and execute
- operator resolution is explicit and auditable
- re-verification after signer or artifact changes updates state
- stale or replayed resolution attempts fail safely

Verification:

- add integration tests for quarantine, resolution, and re-quarantine
- add adversarial tests for stale-state and replay resolution attempts
- `cargo test -p ghost-gateway`

Checkpoint:

- do not proceed into runtime execution until verification and quarantine are
  both enforced under test

### T8. Align Runtime Resolution, Tool Exposure, and Execute Semantics

Deliver:

- runtime resolves external skills only through the catalog
- tool registration derives from the resolved catalog set
- matching `skill:<name>` grants derive from the same resolved set
- execute route resolves through the same catalog-owned seam

Primary files:

- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-gateway/src/api/skill_execute.rs`
- `crates/ghost-gateway/src/skill_catalog/executor.rs`
- `crates/ghost-agent-loop/src/tools/skill_bridge.rs`

Acceptance criteria:

- there is no grant/exposure drift for external skills
- disabled, unverified, or quarantined external skills do not register or
  execute
- install, disable, and quarantine transitions change runtime behavior on the
  next resolution pass
- route execution does not introduce an external-skill bypass

Verification:

- add mixed-source runtime tests for allowlist, always-on, installed,
  disabled, and quarantined cases
- grep check for route or runtime direct-call bypasses
- `cargo test -p ghost-gateway`

### T9. Update the Skill API, OpenAPI, and Generated SDK Types

Deliver:

- truthful API schemas for mixed-source skills
- signer, digest, verification, quarantine, and lifecycle fields in DTOs
- regenerated SDK types for the new contract

Primary files:

- `crates/ghost-gateway/src/api/skills.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`

Acceptance criteria:

- `/api/skills` expresses external-skill truth without compatibility fiction
- mutation and inspection endpoints expose real operator-meaningful fields
- generated types no longer force dashboard or CLI to infer trust state

Verification:

- regenerate types from OpenAPI
- add SDK contract tests for new DTOs
- `cargo test -p ghost-gateway`
- project-specific SDK generation and test commands

### T10. Update the SDK, Dashboard, and CLI Operator Surfaces

Deliver:

- SDK client support for new fields and actions
- dashboard review surfaces for signer, digest, privileges, capabilities,
  verification, quarantine, and rollout state
- CLI inspect/list/install/enable/disable/quarantine flows aligned to API truth

Primary files:

- `packages/sdk/src/skills.ts`
- `packages/sdk/src/__tests__/client.test.ts`
- `dashboard/src/routes/skills/+page.svelte`
- `dashboard/src/components/SkillCard.svelte`
- `crates/ghost-gateway/src/cli/skill.rs`

Acceptance criteria:

- dashboard and CLI can explain why a skill is trusted or blocked
- no surface collapses state into only `skill:<name>`
- dangerous actions are gated or disabled when policy does not allow them
- signer and digest information are visible enough for operator review

Verification:

- add dashboard tests for trust and quarantine rendering
- extend CLI tests for inspect and lifecycle transitions
- run SDK tests
- run dashboard tests for touched skill surfaces

### T11. Implement the WASM Runtime and Restricted Host ABI

Deliver:

- production `wasmtime` execution path
- explicit host ABI definitions
- fuel metering
- memory caps
- wall-clock timeouts
- deterministic execution error mapping
- forensic capture hooks

Primary files:

- `crates/ghost-skills/src/sandbox/wasm_sandbox.rs`
- `crates/ghost-skills/src/sandbox/mod.rs`
- expected new host ABI modules under `crates/ghost-skills/src/sandbox/`
- `crates/ghost-gateway/src/skill_catalog/executor.rs`

Acceptance criteria:

- only explicitly brokered host capabilities are exposed
- raw filesystem, env, subprocess, and unrestricted network access stay denied
- timeout, fuel exhaustion, and memory exhaustion fail closed
- policy and forensic hooks can quarantine a skill after sandbox violations

Verification:

- add WASM isolation tests for denied host capabilities
- add tests for timeout, fuel exhaustion, and memory exhaustion
- add integration tests proving gateway process stability under sandbox failure
- `cargo test -p ghost-skills`
- `cargo test -p ghost-gateway`

Checkpoint:

- do not broaden rollout until WASM failures are contained and auditable
- live checkpoint: the host ABI is narrower than originally sketched and still
  exposes no host imports; broader brokered capabilities remain future work

### T12. Add Adversarial Trust-Boundary and Sandbox Test Suites

Deliver:

- dedicated adversarial fixtures for malformed and malicious artifacts
- trust-boundary test suites
- sandbox-abuse test suites
- restart and persistence failure-mode tests

Primary files:

- `crates/ghost-signing/tests/`
- `crates/ghost-skills/tests/`
- `crates/ghost-gateway/tests/`

Acceptance criteria:

- known bypass classes are covered by committed tests
- tests prove fail-closed behavior under malformed input and resource abuse
- at least one restart-persistence test exists for each lifecycle boundary
- trust and runtime drift regressions are captured before release

Verification:

- run the full Rust test matrix for touched crates
- run targeted adversarial suites introduced by this work
- preserve failing fixtures as regression tests when bugs are found

Checkpoint:

- live checkpoint: adversarial suites now cover hidden imports, host-escape
  probes, tampered managed artifacts, runtime-policy drift, and resource abuse;
  final closeout still requires the full package-wide evidence pass in T14

### T13. Update Docs, Governance, and Rollout Gates

Deliver:

- docs for artifact format, trust model, operator workflow, and WASM limits
- explicit rollout stage documentation
- truthful UX copy for gated vs live behavior

Primary files:

- `docs/skill-pipeline/future-phase.md`
- `agent/AGENT_ARCHITECTURE.md`
- `wiki/ghost-skills.md`
- any new external-skill operator docs added in this phase

Acceptance criteria:

- docs explain exactly what source kinds are supported
- docs explain exact quarantine and verification behavior
- docs explain rollout restrictions and remaining non-goals
- no doc implies a broader trust model than the live product actually enforces

Verification:

- manual doc review
- grep for stale claims such as direct disk execution, implicit trust, or
  unrestricted WASM

### T14. Full Regression Pass and Staged Rollout Evidence

Deliver:

- full test run evidence for Rust, SDK, and dashboard surfaces
- explicit evidence for each rollout gate
- operator sign-off notes on Stage A through Stage D readiness

Verification commands:

- `cargo test -p ghost-signing`
- `cargo test -p ghost-skills`
- `cargo test -p ghost-gateway`
- project-specific SDK test command
- project-specific dashboard test command
- any newly introduced adversarial or soak suites for ingestion and WASM

Acceptance criteria:

- all newly added tests pass
- rollout stage evidence exists before broadening execution
- no known external-skill trust-boundary drift remains in touched surfaces

## Rollout Checkpoints

### Checkpoint A: Artifact and State Foundation Stable

Must be true after T0-T4:

- artifact format is frozen
- ingestion is gateway-owned
- persistent artifact and lifecycle state exists
- no external execution path is live

### Checkpoint B: Trust Enforcement Stable

Must be true after T5-T7:

- catalog truth is mixed-source and authoritative
- signing is cryptographic and digest-bound
- quarantine blocks runtime exposure
- operator actions are auditable

### Checkpoint C: Runtime Alignment Stable

Must be true after T8-T10:

- resolved skill exposure and granted capabilities align
- `/api/skills` is truthful for external skills
- SDK, dashboard, and CLI all consume one contract

### Checkpoint D: WASM Containment Stable

Must be true after T11-T12:

- WASM host ABI is minimal and enforced
- resource abuse fails closed
- sandbox violations produce forensics and quarantine hooks

### Checkpoint E: Governance Stable

Must be true after T13-T14:

- docs match implementation and rollout stage
- evidence exists for all exit criteria

## Negative Cases That Must Be Tested

- install an unsigned skill
- install a skill with an invalid manifest schema version
- install a skill with missing required privileges metadata
- execute a skill that is verified but not installed
- execute a disabled skill
- execute a quarantined skill
- execute a skill signed by a revoked signer
- allowlisted skill without aligned policy-grant regression
- uninstall or disable an always-on compiled skill
- source-file deletion after ingest
- artifact replacement with the same name and version but different digest
- duplicate verification record replay against a new digest
- dashboard review of a skill with dangerous requested capabilities
- CLI inspect/list view of a quarantined skill

## Adversarial Cases That Must Be Tested

- symlink swap during ingestion
- TOCTOU mutation between scan and copy
- archive path traversal and duplicate logical paths
- manifest key-order or whitespace ambiguity affecting signatures
- artifact tampering after signature creation
- stale quarantine-resolution replay after artifact replacement
- signer revocation during installed lifecycle
- runtime resolution drift between tool exposure and `skill:<name>` grant
- direct execute bypass attempt around catalog checks
- WASM import probing for hidden host functions
- WASM infinite loop / fuel exhaustion
- WASM memory-growth exhaustion
- WASM attempts at filesystem, env, subprocess, or raw network access
- restart during verification or quarantine transition

## Task Completion Standard

A task is only complete when:

- code is implemented
- tests proving the intended behavior exist
- adversarial tests exist for the trust boundary touched by that task
- stale behavior or stale copy on the same surface is removed
- restart-persistence behavior is proven for new lifecycle state
- the next dependent task can proceed without relying on undocumented behavior
