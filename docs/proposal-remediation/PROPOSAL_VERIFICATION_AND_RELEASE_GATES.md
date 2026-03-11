# Proposal Verification And Release Gates

Date: March 11, 2026

Status: Required for completion

Purpose: define the verification program and release criteria for the proposal remediation.

## Verification Principles

- Test the real lifecycle semantics, not simplified UI assumptions.
- Validate both creation-time and decision-time behavior.
- Treat stale review, supersession, and offline replay as first-class correctness cases.
- Require parity between gateway contract, SDK types, and ADE behavior.

## Required Test Categories

### 1. Storage and lifecycle tests

Must prove:

- pending human-review proposals resolve to canonical `pending_review`
- auto-approved proposals resolve to canonical `auto_applied`
- superseded proposals cannot remain actionable
- timed-out proposals are terminal and non-pending

Suggested files:

- `crates/cortex/cortex-storage/tests/migration_tests.rs`
- new or existing proposal query tests

### 2. Gateway API tests

Must prove:

- `GET /api/goals?status=pending` returns only pending review proposals
- approved/rejected filters reflect canonical state
- detail contract exposes the same canonical lifecycle field
- stale approval and stale rejection conflicts remain explicit

Suggested files:

- `crates/ghost-gateway/tests/operation_journal_tests.rs`
- new goals API tests if needed

### 3. WebSocket tests

Must prove:

- proposal creation emits the canonical proposal update event
- proposal decision emits the canonical proposal update event
- consumers can keep queue state current from the event stream

Suggested files:

- `crates/ghost-gateway/tests/*websocket*`
- SDK websocket tests

### 4. SDK tests

Must prove:

- goals wrapper matches generated contract
- websocket event union includes proposal-domain live events
- client request behavior preserves idempotency semantics for decisions

Suggested files:

- `packages/sdk/src/__tests__/client.test.ts`
- `packages/sdk/src/__tests__/websocket.test.ts`

### 5. Dashboard tests

Must prove:

- canonical route shows real pending proposals
- new proposals appear live
- stale decision conflicts re-render authoritative truth
- secondary route is redirected or otherwise non-divergent

Suggested files:

- add dedicated Playwright spec for proposals
- update `dashboard/tests/mobile.spec.ts`

### 6. Service worker tests

Must prove one of:

- proposal decisions are blocked offline with correct error messaging

or

- proposal decisions queue and replay with explicit stale conflict handling

## Required Regression Scenarios

These scenarios are mandatory:

1. Human-review proposal created while operator is on the queue page.
2. Auto-approved proposal created while operator is on the queue page.
3. Pending proposal superseded by a newer proposal before operator decision.
4. Operator approves with stale reviewed revision.
5. Operator rejects a proposal already decided in another tab/session.
6. Operator goes offline during proposal decision attempt.
7. Navigation, notifications, and command palette all land on the same canonical route.

## Release Gates

The remediation cannot close until all of these pass:

1. `cargo test` covering proposal-domain backend changes
2. storage/query tests for lifecycle semantics
3. gateway tests for list/detail/decision state correctness
4. SDK tests for goals and websocket parity
5. `pnpm -C dashboard check`
6. dashboard e2e tests covering canonical proposal route
7. any proposal-specific parity or architecture checks added during remediation

## Required Manual Validation

Before closeout:

1. Start the gateway and dashboard.
2. Create a human-review proposal through the live producer path.
3. Verify it appears in the canonical queue without refresh.
4. Open detail view.
5. Approve or reject it.
6. Verify queue/history update live.
7. Repeat with a stale/superseded case.
8. Validate offline behavior for proposal decisions.

## Closeout Evidence

The final closeout document must include:

- exact commands run
- exact suites added or updated
- final canonical route decision
- any intentionally retained compatibility exceptions
- any explicit deferred work with rationale
