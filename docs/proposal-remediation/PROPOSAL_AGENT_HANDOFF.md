# Proposal Remediation Agent Handoff

Date: March 11, 2026

Audience: implementation agent

Purpose: provide direct execution instructions for building the proposal remediation from start to finish.

## Mission

Implement the proposal remediation program defined in this packet so the ADE proposal subsystem becomes one coherent, authoritative, live-updating, production-grade workflow.

Your authority sources, in order:

1. `docs/proposal-remediation/PROPOSAL_MASTER_REMEDIATION_SPEC.md`
2. `docs/proposal-remediation/PROPOSAL_ARCHITECTURE_AND_CONTRACT_MODEL.md`
3. `docs/proposal-remediation/PROPOSAL_IMPLEMENTATION_PLAN.md`
4. live code

If older docs disagree with this packet, follow this packet.

## Hard Constraints

- Do not patch only the UI symptom.
- Do not preserve two divergent proposal review routes.
- Do not use legacy `decision` nullability as proposal lifecycle truth.
- Do not leave proposal creation without live queue wiring.
- Do not rely on optimistic local mutation as final state authority.
- Do not close the work without explicit proposal-domain tests.

## Required Deliverables

You must deliver:

1. one canonical proposal lifecycle contract
2. one canonical ADE proposal route
3. live proposal creation and decision updates
4. server-authoritative approve/reject flow
5. explicit service worker policy for proposal decisions
6. updated tests and release evidence

## Recommended Execution Order

1. Fix backend lifecycle filtering and public status authority.
2. Regenerate and align SDK proposal types.
3. Add canonical proposal WebSocket event coverage for creation and state changes.
4. Consolidate ADE proposal review onto one route.
5. Remove optimistic mutation drift and add authoritative refresh behavior.
6. Decide and implement proposal offline policy.
7. Add and run verification suites.
8. Write a closeout doc summarizing what changed and what remains deferred.

## Minimum Acceptance Criteria

The implementation is not done unless all are true:

- pending queue shows real human-review proposals
- auto-resolved proposals are not shown as pending
- new proposals appear live
- stale decisions show explicit feedback and refreshed truth
- nav/notifications/commands all go to one canonical proposal surface
- tests cover the real state model

## File Targets

Expect to touch at least:

- `crates/ghost-gateway/src/api/goals.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `packages/sdk/src/goals.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`
- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/goals/[id]/+page.svelte`
- `dashboard/src/routes/approvals/+page.svelte`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/components/NotificationPanel.svelte`
- `dashboard/src/service-worker.ts`
- proposal-domain tests across backend, SDK, and dashboard

## Suggested Closeout Format

When implementation is complete, write:

- `PROPOSAL_REMEDIATION_CLOSEOUT.md`

That document should include:

- final canonical route decision
- final canonical lifecycle field decision
- list of incompatible contract changes
- verification commands run
- residual explicitly accepted risks, if any

## Final Instruction

Treat the proposal subsystem as safety- and operator-critical. Prefer deleting ambiguity over preserving compatibility shortcuts.
