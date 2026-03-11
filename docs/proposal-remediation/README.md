# Proposal Remediation Documentation Packet

Date: March 11, 2026

Status: Draft for execution

Purpose: provide an implementation-grade documentation package for remediating the ADE proposal subsystem so it becomes one coherent, authoritative, production-safe system across gateway, SDK, dashboard, WebSocket, service worker, tests, and operator workflow.

This packet is based on the live codebase as reviewed on March 11, 2026. If any older proposal, ADE, dashboard, or architecture document conflicts with this packet, this packet wins for proposal-domain work until superseded by a closeout document.

## Reading Order

1. `PROPOSAL_REMEDIATION_CHARTER.md`
2. `PROPOSAL_MASTER_REMEDIATION_SPEC.md`
3. `PROPOSAL_ARCHITECTURE_AND_CONTRACT_MODEL.md`
4. `PROPOSAL_IMPLEMENTATION_PLAN.md`
5. `PROPOSAL_VERIFICATION_AND_RELEASE_GATES.md`
6. `PROPOSAL_AGENT_HANDOFF.md`

## Document Roles

- `PROPOSAL_REMEDIATION_CHARTER.md`
  - program scope, non-negotiable engineering bar, success definition, and ownership model
- `PROPOSAL_MASTER_REMEDIATION_SPEC.md`
  - authoritative problem statement, confirmed findings, target state, and required behavioral changes
- `PROPOSAL_ARCHITECTURE_AND_CONTRACT_MODEL.md`
  - canonical domain model, API/WebSocket/state authority rules, and cross-component flow
- `PROPOSAL_IMPLEMENTATION_PLAN.md`
  - work packages, sequencing, task breakdown, acceptance criteria, and file-level execution map
- `PROPOSAL_VERIFICATION_AND_RELEASE_GATES.md`
  - test strategy, parity gates, regression matrix, rollout checks, and exit criteria
- `PROPOSAL_AGENT_HANDOFF.md`
  - execution instructions for the implementing agent, including order of operations, hard constraints, and done definition

## Primary Code Inputs

- `dashboard/src/routes/approvals/+page.svelte`
- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/goals/[id]/+page.svelte`
- `dashboard/src/components/NotificationPanel.svelte`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/service-worker.ts`
- `dashboard/tests/mobile.spec.ts`
- `packages/sdk/src/goals.ts`
- `packages/sdk/src/client.ts`
- `packages/sdk/src/websocket.ts`
- `crates/ghost-gateway/src/api/goals.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- `crates/cortex/cortex-storage/src/migrations/v046_goal_proposal_v2.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/proposal/router.rs`
- `crates/ghost-gateway/tests/operation_journal_tests.rs`
- `crates/cortex/cortex-storage/tests/migration_tests.rs`

## Intended Outcome

At the end of this program:

- proposal lifecycle state has one canonical authority model
- the operator sees one canonical proposal surface in ADE
- pending, approved, rejected, superseded, timed-out, and auto-resolved states are represented truthfully everywhere
- new proposal creation and proposal resolution are both live-updating
- offline, idempotency, stale review, and supersession semantics are explicit and tested
- no dashboard, SDK, or gateway path depends on legacy nullability heuristics to infer state
