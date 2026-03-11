# ADE Security Remediation: Workstream Plan

Status: draft for implementation orchestration on March 11, 2026.

This document groups the remediation into orchestrated workstreams so multiple
engineers or agents can execute without stepping on one another.

## Workstream A: Contract Canonicalization

Objective:

- eliminate safety-status and audit-filter ambiguity

Primary scope:

- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-audit/src/query_engine.rs`
- `packages/sdk/src/safety.ts`
- `packages/sdk/src/audit.ts`
- generated SDK types

Outputs:

- canonical safety payload
- canonical audit filter semantics
- canonical severity and event vocabularies

Exit criteria:

- dashboard can render safety state without string inference
- query and export share the same filter semantics

## Workstream B: Security State Orchestration

Objective:

- make the Security route a coherent live operational surface

Primary scope:

- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/lib/stores/safety.svelte.ts`
- `dashboard/src/lib/stores/audit.svelte.ts`
- optional `dashboard/src/lib/stores/security.svelte.ts`

Outputs:

- section-level state model
- consistent refresh behavior
- visible per-agent interventions
- no silent fallbacks

Exit criteria:

- security events update all dependent sections
- section failures remain visible and isolated

## Workstream C: Auth And Affordance Alignment

Objective:

- ensure the ADE never advertises unauthorized security actions

Primary scope:

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/routes/security/+page.svelte`
- `dashboard/src/components/CommandPalette.svelte`
- optional auth session store

Outputs:

- centralized principal state for shell gating
- action gating for `kill-all`
- action gating for sandbox review decisions
- shortcut registration parity

Exit criteria:

- non-authorized principals cannot invoke or see privileged actions in shell or
  page contexts

## Workstream D: Evidence Surface Repair

Objective:

- make filter, timeline, and export behavior evidence-correct

Primary scope:

- `dashboard/src/components/FilterBar.svelte`
- `dashboard/src/components/AuditTimeline.svelte`
- `dashboard/src/routes/security/+page.svelte`
- optional shared constants/helpers

Outputs:

- contract-owned filter options
- canonical severity rendering
- filtered export parity

Exit criteria:

- on-screen and exported evidence sets align
- no fake filter options remain

## Workstream E: Verification Buildout

Objective:

- prove the remediation under normal, denied, and degraded conditions

Primary scope:

- `crates/ghost-gateway/tests/`
- `packages/sdk/src/__tests__/`
- `dashboard/tests/`

Outputs:

- contract tests
- permission tests
- degraded-path tests
- websocket refresh tests

Exit criteria:

- all required automated coverage exists and passes

## Workstream F: Signoff And Closeout

Objective:

- convert verified implementation into signoff-grade delivery evidence

Primary scope:

- closeout summary
- verification evidence
- residual risk statement
- doc updates if any contract changed

Outputs:

- completed `12_SIGNOFF_PACKET.md`
- final engineering briefing

Exit criteria:

- an external engineer can review the packet and evaluate signoff without
  rediscovering the work

## Dependency Order

The intended order is:

1. Workstream A
2. Workstream B and Workstream C
3. Workstream D
4. Workstream E
5. Workstream F

Notes:

- Workstream C may start in parallel with late Workstream B once auth-session
  shape is stable.
- Workstream D must not finalize before Workstream A is frozen.
- Workstream F cannot start before Workstream E is verified.
