# ADE Security Remediation: Implementation Plan

Status: draft for implementation handoff on March 11, 2026.

This document turns the target architecture into a build sequence.

Execution governance for this plan lives in:

- `07_EXECUTION_CONTROL_PLAN.md`
- `08_WORKSTREAM_PLAN.md`
- `09_EXECUTION_TRACKER.md`
- `11_DRIFT_CONTROL_PROTOCOL.md`

## Phase 0: Freeze Contract And Scope

Goal: stop local patching and freeze the remediation target.

Files:

- `docs/ade-security/README.md`
- `docs/ade-security/01_PROBLEM_STATEMENT.md`
- `docs/ade-security/02_TARGET_ARCHITECTURE.md`
- `docs/ade-security/03_CONTRACT_MATRIX.md`
- `docs/ade-security/04_IMPLEMENTATION_PLAN.md`
- `docs/ade-security/05_VERIFICATION_AND_RELEASE_GATES.md`
- `docs/ade-security/06_AGENT_HANDOFF.md`

Acceptance criteria:

- this package exists and is internally cross-linked
- implementation work references this package rather than ad hoc notes
- execution state is tracked in `09_EXECUTION_TRACKER.md`

## Phase 1: Canonicalize Safety And Audit Contracts

Goal: remove contract ambiguity before UI rewiring.

Primary files:

- `crates/ghost-gateway/src/api/safety.rs`
- `packages/sdk/src/safety.ts`
- `packages/sdk/src/generated-types.ts`
- `crates/ghost-audit/src/query_engine.rs`
- `packages/sdk/src/audit.ts`

Changes:

1. Add canonical safety-status fields usable without string parsing.
2. Decide and implement audit filter semantics:
   either repeated query params / set-based filtering or explicit single-select.
3. Freeze canonical event-type and severity vocabularies.
4. Ensure export accepts the same filter model as query.

Acceptance criteria:

- the frontend can render safety state without enum-name parsing
- audit filter semantics are explicit and testable
- SDK types match gateway payloads

## Phase 2: Build A Cohesive Security Orchestration Layer

Goal: ensure the Security route behaves as one live surface.

Primary files:

- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/lib/stores/audit.svelte.ts`
- `dashboard/src/lib/stores/safety.svelte.ts`
- optional new store:
  `dashboard/src/lib/stores/security.svelte.ts`

Changes:

1. Replace route-local fragmented loading logic with one orchestration layer or a
   disciplined route controller.
2. Add section-level load, empty, and error states.
3. Refresh audit evidence on security events and resync.
4. Surface per-agent interventions and distributed kill / convergence
   protection.
5. Remove any failure-to-empty fallback.

Acceptance criteria:

- security events keep all sections in sync
- each section can fail independently and visibly
- per-agent intervention state is visible

## Phase 3: Align UI Affordances With Authorization

Goal: no action appears without corresponding privilege.

Primary files:

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`
- optional new auth state store:
  `dashboard/src/lib/stores/auth-session.svelte.ts`
- `packages/sdk/src/auth.ts`

Changes:

1. Centralize auth session state for shell-wide gating.
2. Gate the `kill-all` button.
3. Gate sandbox review decision controls.
4. Gate global shortcut registration.
5. Gate command-palette actions if they surface the same privileged controls.

Acceptance criteria:

- unauthorized principals do not see or cannot invoke privileged controls
- no shell-level privileged action remains registered without permission

## Phase 4: Repair Filter And Timeline Rendering

Goal: the audit surface returns and renders the real evidence set.

Primary files:

- `dashboard/src/components/FilterBar.svelte`
- `dashboard/src/components/AuditTimeline.svelte`
- `dashboard/src/routes/security/+page.svelte`
- optional shared constants file:
  `dashboard/src/lib/security-contract.ts`

Changes:

1. Replace page-local event/severity values with contract-owned values.
2. Align filter UI with backend semantics.
3. Align timeline severity rendering with canonical severity tokens.
4. Make export use the active filter state.

Acceptance criteria:

- every visible filter option can return real data
- every canonical severity renders correctly
- exported evidence matches on-screen filtered evidence

## Phase 5: Test The Full Surface

Goal: prove the remediation works under normal, denied, and degraded states.

Primary files:

- `dashboard/tests/`
- `packages/sdk/src/__tests__/`
- `crates/ghost-gateway/tests/`

Required tests:

1. Safety-status contract and SDK parity tests.
2. Audit filter semantics tests.
3. Security page Playwright tests for:
   - authorized principal
   - unauthorized principal
   - degraded sandbox review fetch
   - live refresh after websocket events
   - export matches active filters
4. Shell shortcut tests for non-registration when unauthorized.

Acceptance criteria:

- all required tests exist and pass
- release gates in the verification doc pass

## Phase 6: Cutover And Closeout

Goal: remove temporary compatibility logic only when the surface is proven.

Changes:

1. Remove temporary parsing bridges if introduced.
2. Remove deprecated frontend fallback code.
3. Update any architecture docs that still describe the old behavior.

Acceptance criteria:

- no dead compatibility branches remain without a migration reason
- security route behavior is consistent with the package contracts

## Implementation Order Constraints

- do not patch UI parsing before freezing the gateway/SDK contract
- do not ship page gating without shell gating
- do not ship filter UI changes without export alignment
- do not ship the remediation without automated tests
