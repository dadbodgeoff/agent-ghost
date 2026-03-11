# Proposal Implementation Plan

Date: March 11, 2026

Status: Execution plan

Purpose: break the proposal remediation into implementation-grade work packages with explicit sequencing and acceptance criteria.

## Execution Strategy

The implementation should proceed from authority layers outward:

1. persistence and API truth
2. SDK contract alignment
3. WebSocket event model
4. ADE route consolidation
5. service worker policy
6. tests and release gates

Do not start by patching `/approvals` in isolation. The root defect is contract drift, not just a UI bug.

## Work Package 1: Canonical lifecycle API

Objective:

- make `GET /api/goals` and `GET /api/goals/{id}` publish one authoritative lifecycle status

Primary files:

- `crates/ghost-gateway/src/api/goals.rs`
- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/goals.ts`

Tasks:

1. Define canonical public lifecycle status field.
2. Update list query logic to filter by transition-derived state.
3. Update detail payload to expose the same state authority.
4. Preserve legacy fields only if needed for compatibility, but stop using them to drive filtering.
5. Regenerate SDK types and align wrapper types to generated output.

Acceptance criteria:

- pending API results contain only `pending_review`
- auto-resolved states do not leak into pending
- detail and list use the same lifecycle vocabulary

## Work Package 2: Proposal event contract

Objective:

- make proposal creation and state transitions live-updating

Primary files:

- `crates/ghost-gateway/src/api/websocket.rs`
- proposal creation path in `crates/ghost-agent-loop/src/runner.rs`
- dashboard WebSocket consumers
- `packages/sdk/src/websocket.ts`

Tasks:

1. Introduce proposal creation or generic proposal update event.
2. Emit event on proposal creation.
3. Emit event on terminal state changes.
4. Update SDK typed WebSocket union.
5. Update dashboard store and proposal route consumers.

Acceptance criteria:

- a newly created proposal appears live in the queue
- a decided proposal exits pending and enters history live

## Work Package 3: Canonical ADE route

Objective:

- collapse proposal review onto one authoritative ADE surface

Primary files:

- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/goals/[id]/+page.svelte`
- `dashboard/src/routes/approvals/+page.svelte`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/components/NotificationPanel.svelte`
- `dashboard/src/components/CommandPalette.svelte`

Tasks:

1. Choose canonical route.
2. Move nav/notifications/commands to that route.
3. Merge best behavior from both existing surfaces.
4. Remove or redirect the secondary surface.
5. Ensure detail and list share one status model and one decision path.

Acceptance criteria:

- only one route is authoritative
- the route supports queue, history, detail, and decision
- no duplicate review logic remains

## Work Package 4: Server-authoritative mutation flow

Objective:

- eliminate optimistic mutation drift

Primary files:

- canonical proposal list/detail route
- shared proposal action controller if created

Tasks:

1. Add in-flight row locking or action disablement.
2. Fetch fresh preconditions before decision if necessary.
3. Refetch authoritative list/detail after success.
4. Refetch authoritative state after stale conflict.
5. Show typed stale/superseded operator messaging.

Acceptance criteria:

- double-clicking does not corrupt UI state
- stale conflicts refresh to truth
- resolver/status/history are correct after action

## Work Package 5: Service worker policy

Objective:

- make offline proposal decision behavior explicit and safe

Primary files:

- `dashboard/src/service-worker.ts`
- proposal UI messaging paths

Tasks:

1. Decide whether proposal decisions are queueable offline.
2. If not queueable, block them with explicit UX.
3. If queueable, add proposal-specific replay handling and tests.
4. Ensure auth/session rotation invalidates queued proposal decisions appropriately.

Acceptance criteria:

- offline proposal decisions have a documented and tested behavior
- replay cannot silently produce wrong operator expectations

## Work Package 6: Test and gate hardening

Objective:

- make proposal correctness durable

Primary files:

- `dashboard/tests/mobile.spec.ts`
- proposal-specific Playwright tests to add
- `crates/ghost-gateway/tests/operation_journal_tests.rs`
- relevant storage and SDK tests

Tasks:

1. Replace `decision: null` pending mocks with real pending-state fixtures.
2. Add dashboard tests for pending queue truth.
3. Add dashboard tests for new proposal live arrival.
4. Add backend tests for lifecycle-filter correctness.
5. Add tests for canonical route redirect/ownership.
6. Add service worker proposal decision tests or explicit block tests.

Acceptance criteria:

- tests fail if pending is inferred from nullability
- tests fail if live proposal creation is not wired
- tests fail if canonical route ownership regresses

## Suggested File-Level Changes

### Backend

- `crates/ghost-gateway/src/api/goals.rs`
  - replace legacy pending/history filtering logic
  - normalize public lifecycle status
- `crates/ghost-gateway/src/api/websocket.rs`
  - add proposal creation/update event contract
- `crates/ghost-agent-loop/src/runner.rs`
  - emit proposal creation event through gateway integration point if available

### SDK

- `packages/sdk/src/goals.ts`
  - remove wrapper drift and align to generated types
- `packages/sdk/src/websocket.ts`
  - extend typed event union for proposal creation/update events

### Dashboard

- `dashboard/src/routes/goals/+page.svelte`
  - promote to canonical route or absorb best parts of `/approvals`
- `dashboard/src/routes/goals/[id]/+page.svelte`
  - ensure detail route matches canonical list semantics
- `dashboard/src/routes/approvals/+page.svelte`
  - remove, redirect, or collapse into shared implementation
- `dashboard/src/routes/+layout.svelte`
  - route nav to canonical proposal surface
- `dashboard/src/components/NotificationPanel.svelte`
  - point to canonical route and distinguish creation vs decision notifications
- `dashboard/src/service-worker.ts`
  - explicit proposal decision offline policy

## Sequence Constraints

The agent implementing this plan must observe:

1. Do not change dashboard pending logic before changing or explicitly accounting for API semantics.
2. Do not add WebSocket consumers before the event contract is defined.
3. Do not keep `/approvals` and `/goals` alive with divergent state logic after the remediation is complete.
4. Do not leave service worker behavior implicit for proposal decisions.

## Done Definition

This plan is complete only when all work packages have passed the gates in `PROPOSAL_VERIFICATION_AND_RELEASE_GATES.md`.
