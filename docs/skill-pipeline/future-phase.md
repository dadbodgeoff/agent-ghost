# Skill Pipeline Future Phase Plan

## Intent

This document plans the follow-on phase after the compiled-skill hardening
workstream.

The current wave made the live compiled-skill system truthful and coherent.
This document defines the separate future program required to add:

- real file-backed and community skill loading
- real signing and quarantine enforcement
- real untrusted WASM execution

This is intentionally a separate workstream. The current production system must
remain truthful while this work is designed and landed.

## Verified Current Baseline

The following current-state facts were re-verified before writing this plan:

- [registry.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-skills/src/registry.rs)
  exists as a legacy prototype and now fails closed by default unless a caller
  injects an explicit verifier; it is not the external-skill trust root.
- [wasm_sandbox.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-skills/src/sandbox/wasm_sandbox.rs)
  now executes untrusted WASM through `wasmtime` with timeout, fuel, and
  memory limits plus default-deny import handling.
- [native_sandbox.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-skills/src/sandbox/native_sandbox.rs)
  provides only local capability checks.
- The live runtime authority is the gateway-owned catalog in
  [crates/ghost-gateway/src/skill_catalog](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/skill_catalog),
  not the file-backed registry prototype.
- Gateway-owned persistence now stores external artifact identity,
  verification, quarantine, and install state, and `/api/skills` projects that
  mixed-source truth through the catalog.
- External WASM execution now exists only through the gateway-owned catalog
  path, never from workspace roots or raw file paths.

Conclusion:

- the future phase must not "turn on" the prototype modules as-is
- the future phase must generalize the gateway catalog architecture
- file-backed skill support must be introduced through ingestion, persistence,
  verification, and catalog resolution, not direct disk execution

## Current Checkpoint

The repo is now past the original baseline in a few important ways:

- artifact packaging, deterministic hashing, signature verification, managed
  ingestion storage, and persistent lifecycle tables exist
- the gateway catalog lists compiled and external artifacts together with
  digest, signer, verification, quarantine, install, and runtime-visible
  fields
- install and execute flows now resolve external identifiers through the
  catalog and fail closed when verification, quarantine, or runtime support
  is not satisfied
- operator quarantine, stale-revision-safe resolution, and managed-artifact
  reverification are live through the gateway, CLI, SDK, and dashboard
- verified, installed, unquarantined external WASM can now become
  runtime-visible when `external_skills.execution_enabled` is enabled, the
  skill name does not collide with a compiled skill, and the manifest requests
  no host capabilities
- external WASM execution rereads the managed artifact on each execute,
  revalidates its digest, and quarantines on tamper or sandbox-escape evidence
- hidden imports for env, filesystem, subprocess, or network access are denied
  and quarantined; timeout, fuel exhaustion, and memory exhaustion fail closed
  without broadening trust

Still intentionally gated:

- no brokered host capabilities are exposed to external WASM yet
- external WASM must use the current minimal guest ABI (`memory`, `alloc`,
  `run`) and import no host functions
- manifests that request host capabilities remain blocked from runtime
- broader rollout evidence and governance closeout still remain

## Program Objective

Build a production-grade external skill pipeline in which a file-backed or
community-provided skill can only become runtime-visible after all of the
following are true:

1. the artifact is ingested into gateway-managed state
2. its manifest and payload structure are valid
3. its signature and signer trust chain are verified
4. quarantine policy has either not triggered or has been explicitly resolved
5. its execution mode and privileges are representable by the canonical catalog
6. the runtime can execute it through the same catalog-owned policy seam as
   compiled skills

## Non-Negotiable Invariants

### Invariant 1: The Gateway Catalog Remains Canonical

No runtime path may execute a file-backed skill directly from a workspace or
user directory.

All skill sources must be projected into one catalog that owns:

- existence
- source
- version
- verification state
- quarantine state
- install state
- runtime availability
- policy capability
- declared privileges
- execution handle

### Invariant 2: Installation, Verification, and Exposure Are Separate States

The system must not collapse these concepts into one boolean.

A skill can be:

- discovered but not ingested
- ingested but invalid
- valid but untrusted
- verified but not installed
- installed but disabled
- installed and runtime-visible
- quarantined after install due to later evidence

### Invariant 3: Runtime Resolution Must Stay Single-Path

The same resolved catalog set must drive:

- tool registration
- policy grants
- install and uninstall semantics
- `POST /api/skills/{name}/execute`
- dashboard, SDK, and CLI skill state

### Invariant 4: Signing and Quarantine Must Be Enforced, Not Cosmetic

If the UI or API says a skill is quarantined or verification failed, that skill
must not execute.

### Invariant 5: WASM Host Access Must Be Deliberately Small

Untrusted WASM skills must not receive:

- raw filesystem access
- raw environment variable access
- raw subprocess access
- direct unrestricted network access
- raw secrets

They must use brokered host capabilities only.

## Delivery Strategy

Do this in six milestones.

Do not start with WASM. Start with artifacts, ingestion, and state.

## Milestone 0: RFC and ADR Set

Produce an explicit design package before coding:

- skill artifact format ADR
- signer trust model ADR
- quarantine policy ADR
- WASM host ABI ADR
- rollout and compatibility ADR

Exit criteria:

- one artifact format chosen
- one signer trust-root model chosen
- one quarantine state machine agreed
- one runtime ABI boundary agreed

## Milestone 1: Artifact and Manifest Model

Define the only supported external-skill package format.

### Deliverables

- `SkillManifestV1`
- `SkillArtifact`
- `SkillSignatureEnvelope`
- deterministic packaging rules
- canonical hashing rules
- manifest validation library

### Required Manifest Fields

- `name`
- `version`
- `publisher`
- `description`
- `source_kind`
- `execution_mode`
- `entrypoint`
- `requested_capabilities`
- `declared_privileges`
- `content_digests`
- `signature`
- `manifest_schema_version`

### Design Rules

- signatures cover normalized manifest plus content digests
- package hashing is deterministic across machines
- privileges are human-readable and mandatory
- requested host capabilities are machine-readable and mandatory

Exit criteria:

- two independent package builds of the same source yield identical digests
- manifest validation rejects unknown schema versions and missing required
  fields

## Milestone 2: Gateway Ingestion and Persistent State

Replace direct "discovery" thinking with ingestion.

### Deliverables

- ingestion scanner for approved roots
- gateway-managed artifact storage
- persistent skill source/version records
- persistent verification records
- persistent quarantine records
- persistent install-state records for non-compiled sources

### State Machine

Recommended states:

- `discovered`
- `ingested`
- `validation_failed`
- `verification_failed`
- `quarantined`
- `verified`
- `installed`
- `disabled`

### Data Model

Add tables for:

- skill artifacts
- skill versions
- verification results
- quarantine decisions
- signer identities and trust roots
- install state by skill version

Design rule:

- runtime never executes from `~/.ghost/skills` or workspace roots directly
- those locations are only ingestion inputs

Exit criteria:

- deleting a source file after ingestion does not invalidate stored truth
- re-scan updates catalog state through ingestion logic, not ad hoc file reads

## Milestone 3: Catalog Generalization

Extend the gateway catalog so compiled and external skills share one authority.

### Deliverables

- generalized `SkillDefinition` / `ResolvedSkill` model
- catalog support for `compiled`, `user`, and `workspace` sources
- verification and quarantine state surfaced in DTOs
- install/uninstall/enable/disable semantics for external skills

### Architectural Rule

Compiled skills and external skills must converge into the same catalog view.

No separate "compiled API" vs "external skill API" split.

Exit criteria:

- `/api/skills` can truthfully list mixed-source skills
- runtime resolution can include verified external skills and exclude
  unverified or quarantined skills
- dashboard, SDK, and CLI consume the same DTO contract

## Milestone 4: Real Signing and Quarantine Enforcement

Turn verification into an enforced trust boundary.

### Deliverables

- real `ghost-signing` integration
- signer trust-root configuration
- signer revocation support
- quarantine reason taxonomy
- quarantine resolution workflow for operators
- audit emission for ingest, verify, quarantine, and resolution transitions

### Quarantine Triggers

At minimum:

- malformed manifest
- digest mismatch
- invalid signature
- unknown signer
- revoked signer
- unsupported requested capability
- unsupported execution mode
- repeated runtime sandbox violations

### Required Operator Surfaces

- exact verification result
- signer identity
- digest / artifact fingerprint
- quarantine reason
- last transition actor and timestamp

Exit criteria:

- a tampered artifact is quarantined and not executable
- a revoked signer blocks future execution
- quarantine state changes runtime visibility deterministically

## Milestone 5: WASM Runtime and Host ABI

Only start this after Milestones 1 through 4 are real.

### Deliverables

- production `wasmtime` execution path
- fuel / instruction metering
- memory cap
- wall-clock timeout
- explicit guest ABI and default-deny host boundary
- forensic capture on violations
- deterministic error model back to gateway APIs

### Current Live Host Boundary

The live product currently exposes no host-function imports to external WASM.
The only supported ABI is guest-side:

- exported linear `memory`
- exported `alloc(len) -> ptr`
- exported `run(input_ptr, input_len) -> packed_output_ptr_len`

Any import request is treated as a sandbox escape attempt and is denied by
default.

### Candidate Future Host Capability Classes

If the host ABI broadens later, it must start from explicit brokered surfaces
such as:

- structured storage calls
- brokered HTTP through gateway policy
- brokered secret handles
- logging / telemetry

### Explicitly Forbidden Direct Access

- raw filesystem
- raw env vars
- raw sockets
- subprocess launch
- unrestricted timers / background tasks

### Security Rule

WASM skills still execute through the catalog-owned executor seam.

The executor chooses an execution backend:

- compiled native
- verified native external, if ever allowed
- verified WASM

Exit criteria:

- a denied host capability fails closed
- an escape attempt emits forensics and quarantines the skill
- resource exhaustion does not crash the gateway process

## Milestone 6: Rollout and Blast-Radius Control

Roll out in stages behind explicit gates.

### Stage A

Ingestion only.

- no execution
- verification and quarantine visible in API/UI

### Stage B

Verified artifacts can be installed but not executed unless runtime execution is
explicitly enabled and the external runtime contract is satisfied.

### Stage C

Restricted execution.

- gateway-managed verified artifacts only
- no raw host imports
- no brokered host capabilities yet

### Stage D

Broader trust model, only after operational evidence is clean.

## Required Test Matrix

The future phase is not complete without all of these:

### Artifact and Ingestion

- duplicate package ingest resolution
- digest stability
- manifest schema validation
- package update and rollback behavior

### Verification and Quarantine

- missing signature rejected
- invalid signature quarantined
- unknown signer quarantined
- revoked signer blocked
- quarantine resolution audit trail

### Catalog and Runtime

- install changes runtime exposure
- uninstall removes runtime exposure
- resolved skill set equals granted capability set
- quarantined skills do not register as tools
- disabled skills cannot execute through REST or runtime

### WASM Isolation

- no env access
- no filesystem escape
- no subprocess spawn
- no unbrokered network
- timeout enforcement
- fuel enforcement
- memory-cap enforcement
- forensic capture on violation

### Surface Consistency

- dashboard shows signer, privileges, and quarantine reason truthfully
- SDK DTOs match OpenAPI
- CLI inspect/list/install flows reflect the same state

## Non-Goals For This Future Phase

Even this future phase should stay narrow.

Out of scope:

- public marketplace economics
- autonomous approval of community skills
- arbitrary native plugin loading without signing
- "just trust local disk" developer shortcuts in production paths

## Go / No-Go Rules

Do not ship external skill execution if any of the following are still false:

- verification can be bypassed
- quarantine does not block runtime exposure
- runtime policy grants drift from resolved skill exposure
- dashboard cannot show why a skill is trusted
- WASM host ABI still exposes raw machine capabilities

## Recommended PR Sequence

Land the work in three PR groups.

### Group 1

Artifact model, ingestion, persistent state, no execution.

### Group 2

Catalog generalization, signing, verification, quarantine, UI/API truth.

### Group 3

WASM backend, host ABI, runtime rollout gates, forensic handling.

## Completion Standard

This future phase is complete only when an operator can answer all of these
questions from the live system without reading source code:

- where did this skill come from
- what exact artifact is installed
- who signed it
- why is it trusted
- why is it quarantined, if quarantined
- what privileges it can exercise
- whether it is actually executable right now

If the live product cannot answer those questions, the phase is not done.
