# Proposal Master Remediation Spec

Date: March 11, 2026

Status: Draft

Purpose: define the authoritative remediation plan for the ADE proposal subsystem based on the live code.

## Standard

The proposal system must operate as one contract, one operator workflow, and one lifecycle truth across all layers.

## Scope

This spec covers:

- proposal persistence semantics
- proposal lifecycle state interpretation
- proposal REST list/detail/decision endpoints
- proposal WebSocket events
- proposal SDK wrappers
- ADE proposal review UX
- service worker behavior for proposal decisions
- test and release gates for proposal-domain work

## Primary Sources

- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/proposal/router.rs`
- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- `crates/cortex/cortex-storage/src/migrations/v046_goal_proposal_v2.rs`
- `crates/ghost-gateway/src/api/goals.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `packages/sdk/src/goals.ts`
- `packages/sdk/src/client.ts`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/routes/approvals/+page.svelte`
- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/goals/[id]/+page.svelte`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/components/NotificationPanel.svelte`
- `dashboard/src/service-worker.ts`
- `dashboard/tests/mobile.spec.ts`

## Confirmed Findings

### F1. Pending review semantics are split between legacy projection and v2 lifecycle state.

The system currently stores proposal truth in two overlapping shapes:

- legacy projection: `goal_proposals.decision`, `goal_proposals.resolved_at`
- canonical lifecycle path: `goal_proposal_transitions` with latest `current_state`

These are not treated as one authority model in the ADE.

Implication:

- different consumers infer state differently
- pending/historical classification can disagree by surface
- future changes can silently regress operator correctness

Evidence:

- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- `crates/ghost-gateway/src/api/goals.rs`
- `dashboard/src/routes/approvals/+page.svelte`
- `dashboard/src/routes/goals/+page.svelte`

### F2. The `/approvals` surface is wired against the wrong pending-state assumption.

`/approvals` treats pending as `decision === null && resolved_at === null`. The producer persists human-review proposals as `decision = "HumanReviewRequired"`.

Implication:

- the canonical operator queue can be empty while actionable review work exists
- pending proposals are misrouted to History
- the proposal tab is not trustworthy

Evidence:

- `dashboard/src/routes/approvals/+page.svelte`
- `crates/ghost-agent-loop/src/runner.rs`

### F3. The backend list filter is still keyed to legacy columns rather than lifecycle state.

`GET /api/goals` determines pending via `resolved_at IS NULL` and uses raw `decision` values for approved/rejected filters.

Implication:

- auto-resolved states can leak into pending query results
- list semantics are not aligned to the lifecycle engine
- the API itself is not a safe authority for proposal state filtering

Evidence:

- `crates/ghost-gateway/src/api/goals.rs`
- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`

### F4. Proposal creation is not live-wired into operator review.

The gateway emits `ProposalDecision` but not a creation or generic proposal state-change event. ADE review surfaces therefore do not learn about new reviewable proposals in real time.

Implication:

- the operator queue is incomplete without refresh
- notifications are resolution-only rather than queue-forming
- human review can lag behind actual system state

Evidence:

- `crates/ghost-gateway/src/api/websocket.rs`
- `dashboard/src/routes/approvals/+page.svelte`
- `dashboard/src/components/NotificationPanel.svelte`

### F5. The ADE exposes duplicate proposal review surfaces with divergent behavior.

The dashboard ships both `/goals` and `/approvals`.

- `/goals` has better stale-decision handling and reload behavior
- `/approvals` is the nav- and notification-linked route
- `/goals/[id]` is the only real detail page

Implication:

- operator workflows are fragmented
- fixes in one surface do not guarantee correctness in the other
- documentation and testing burden is doubled

Evidence:

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/approvals/+page.svelte`
- `dashboard/src/components/NotificationPanel.svelte`

### F6. The proposal queue uses optimistic mutation where server revalidation is required.

`/approvals` mutates local row state immediately after approve/reject, does not disable the active row, and does not refetch canonical state after the server response.

Implication:

- duplicate clicks can produce extra decision attempts
- stale review conflicts are not surfaced as a first-class operator flow
- detail panels can display stale resolver/state/history data

Evidence:

- `dashboard/src/routes/approvals/+page.svelte`

### F7. Proposal-domain service worker behavior is generic, not proposal-safe.

Proposal decisions are routed through generic mutation replay. That is acceptable only if stale-review, auth, idempotency, and conflict behavior are explicit and tested for proposal mutations specifically.

Implication:

- a superficially “queued” decision may replay into a stale or superseded proposal
- user feedback and queue invalidation behavior may be insufficiently proposal-specific

Evidence:

- `dashboard/src/service-worker.ts`
- `packages/sdk/src/client.ts`
- `crates/ghost-gateway/tests/operation_journal_tests.rs`

### F8. Frontend tests currently encode the wrong happy-path data shape.

The dashboard mock suite uses `decision: null` for pending proposals and contains no dedicated `/approvals` correctness coverage.

Implication:

- tests are validating a false contract
- the exact bug in `/approvals` can pass indefinitely

Evidence:

- `dashboard/tests/mobile.spec.ts`

## Target State

The remediated system must adopt these rules:

### T1. One canonical lifecycle field

Proposal lifecycle truth for all read paths is derived from the latest transition state.

Canonical states:

- `pending_review`
- `approved`
- `rejected`
- `superseded`
- `timed_out`
- `auto_applied`
- `auto_rejected`

Legacy fields may continue to exist for migration compatibility, but they are not lifecycle authority.

### T2. One canonical operator surface

ADE must expose one canonical proposal review route.

That route owns:

- pending queue
- historical queue
- detail drill-in
- decision actions
- stale/superseded handling
- queue live updates

The secondary route, if temporarily retained, must be a redirect or a thin wrapper over the same underlying components and state model.

### T3. One canonical list contract

`GET /api/goals` must filter and sort using canonical lifecycle state.

Required semantics:

- pending filter returns only `pending_review`
- approved filter returns only `approved`
- rejected filter returns only `rejected`
- history excludes currently pending items
- list payload exposes explicit canonical status

### T4. One canonical live-update model

WebSocket must deliver proposal-domain events sufficient for the operator queue to stay current without refresh.

Minimum acceptable event model:

- `ProposalCreated`
- `ProposalStateChanged`

or

- one unified `ProposalUpdated` event that covers both creation and state transitions

### T5. Mutation authority is server-first

Operator approve/reject flows must:

- fetch or already hold fresh review preconditions
- disable in-flight actions on the active proposal
- wait for server confirmation
- refetch authoritative state after success or stale conflict

### T6. Proposal offline replay is explicit

If proposal decisions remain queueable offline, the UX and tests must prove:

- stale replay is surfaced clearly
- superseded replay is dropped or reported correctly
- duplicate replay cannot double-apply a decision

If that bar is too high, proposal decisions should be excluded from offline queuing.

## Required Behavioral Changes

1. Replace all pending/history classification logic based on `decision === null` with canonical lifecycle status.
2. Replace API filtering based on `resolved_at` and raw legacy decision semantics with transition-derived state.
3. Introduce live creation/state-change events for proposals.
4. Consolidate proposal review into one ADE route and one shared component set.
5. Convert proposal decisions to a server-authoritative refresh flow.
6. Update navigation, notification, and command entry points to the canonical route.
7. Update test fixtures to use real pending-state semantics.
8. Add proposal-specific service worker/replay coverage or explicitly disable proposal decision queuing offline.

## Exit Criteria

This remediation is complete only when:

- a real `HumanReviewRequired` proposal appears in the pending operator queue
- a new proposal appears live without refresh
- a superseded proposal disappears from pending and shows truthful history
- stale decision attempts produce deterministic operator feedback
- only one proposal review route is authoritative in ADE
- tests and gates fail if legacy-nullability assumptions reappear
