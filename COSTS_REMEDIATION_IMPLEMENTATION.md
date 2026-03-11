# Costs Remediation Implementation

Status: March 11, 2026

Authoritative design: `COSTS_MASTER_REMEDIATION_SPEC.md`

Purpose: translate the master spec into an exact implementation shape with file-level work, sequencing, and acceptance checks.

## 0. Build Strategy

This remediation should be implemented as one coordinated program, but in dependency order:

1. Fix backend truth and enforcement.
2. Fix backend contracts and live event propagation.
3. Unify dashboard consumption paths.
4. Normalize cache policy.
5. Add tests and gates.

The order matters because the UI must not be refactored around a contract that is still wrong.

## 1. Backend Ledger and Enforcement

### 1.1 Seed runtime spending from `CostTracker`

Files:

- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/context/run_context.rs`

Required change:

- When building the runner for a specific agent, set `runner.daily_spend` from `deps.cost_tracker.get_daily_total(ctx.agent.id)`.
- Do not rely on runner-local default `0.0` for same-day spending state.
- Keep `ctx.total_cost` as per-run accumulation only.

Invariant:

- `runner.daily_spend + ctx.total_cost` must equal "same-day pre-run ledger + current-run incremental spend."

### 1.2 Add explicit UTC rollover task

Files:

- `crates/ghost-gateway/src/cost/tracker.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- optionally `crates/ghost-gateway/src/state.rs` if a helper is warranted

Required change:

- Add an explicit background task that detects UTC date change.
- On UTC day rollover:
  - clear in-memory agent daily totals
  - clear in-memory compaction totals
  - keep session totals only if they are defined as cross-day session totals; otherwise clear them too
  - emit a typed websocket reset event

Recommended implementation shape:

- Store the currently-active UTC date string in the rollover task loop.
- Wake every 30-60 seconds.
- When the current UTC day changes, call a tracker reset method and emit `CostDailyReset`.

Important design note:

- Do not bury day rollover inside a read path or `record()` call.
- Day rollover is operational state transition and should be explicit, observable, and testable.

### 1.3 Keep session totals authoritative

Files:

- `crates/ghost-gateway/src/cost/tracker.rs`
- `crates/ghost-gateway/src/api/sessions.rs`

Required change:

- Replace the heuristic `token_count * 0.000003` computation in session events with `state.cost_tracker.get_session_total(session_uuid)`.
- Parse the session id once, fail gracefully if malformed, and use the tracked ledger total.
- Preserve pagination for events while decoupling cost from page boundaries.

Important rule:

- `SessionEventsResponse.cumulative_cost` is a ledger field, not a local fold over the returned `events` array.

### 1.4 Keep cost recording on the runtime hot path, but emit domain events

Files:

- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- optionally `crates/ghost-gateway/src/api/openapi.rs` if shared schemas are used for websocket docs/comments

Required change:

- Extend the runtime cost recorder closure so it does both:
  - `cost_tracker.record(...)`
  - emit a websocket `CostUpdate` event with post-record totals

The event payload should include:

- `agent_id`
- `session_id`
- `daily_total`
- `session_total`
- `compaction_cost`
- `spending_cap`
- `cap_remaining`
- `cap_utilization_pct`
- `is_compaction`

The event is emitted after recording so it always represents the post-mutation state.

## 2. REST and SDK Contract Alignment

### 2.1 Keep `/api/costs` truthful and boring

Files:

- `crates/ghost-gateway/src/api/costs.rs`
- `packages/sdk/src/costs.ts`
- `packages/sdk/src/generated-types.ts`

Required change:

- Preserve the current response shape unless additional fields are strictly necessary.
- Ensure all derived values are calculated from the tracker-backed `daily_total`.
- Keep one SDK type: `AgentCostInfo`.

No duplicate dashboard-side domain interface is allowed after this remediation.

### 2.2 Type websocket cost events in the SDK

Files:

- `crates/ghost-gateway/src/api/websocket.rs`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/index.ts`
- tests covering websocket event unions

Required change:

- Add the new cost events to the backend enum.
- update the SDK websocket event union so dashboard consumers do not cast through untyped local shapes
- export those event types from the SDK if current patterns do so for other websocket events

## 3. Dashboard Unification

### 3.1 Convert the costs store into the single dashboard cost authority

Files:

- `dashboard/src/lib/stores/costs.svelte.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`

Required change:

- Replace the local `AgentCost` interface with `AgentCostInfo` from `@ghost/sdk`.
- Add handlers for:
  - `CostUpdate`
  - `CostDailyReset`
  - `Resync`
- Make the store reconcile updates by `agent_id`.

Recommended behavior:

- `CostUpdate`: patch or insert the affected agent row in-place.
- `CostDailyReset`: either clear local data or force immediate `refresh()`.
- `Resync`: full `refresh()`.

### 3.2 Make `/costs` consume the shared store

Files:

- `dashboard/src/routes/costs/+page.svelte`

Required change:

- Remove the route-local fetch path.
- Initialize and consume `costsStore`.
- Keep formatting helpers local if needed, but not the data source.

Completion rule:

- there must be exactly one dashboard data-loading path for the global costs view

### 3.3 Align the agent detail page with the shared store or shared refresh path

Files:

- `dashboard/src/routes/agents/[id]/+page.svelte`

Required change:

- The agent detail cost card must either:
  - subscribe to the shared costs store and select by `agent_id`, or
  - participate in the same cost refresh/event model as the global costs page

Preferred approach:

- reuse the shared costs store to avoid drift in formatting and freshness logic

### 3.4 Make session detail cost truthful

Files:

- `dashboard/src/routes/sessions/[id]/+page.svelte`

Required change:

- Keep the session cost display, but ensure it is sourced from the corrected backend `cumulative_cost`.
- Add refresh behavior on relevant live events if the page is intended to stay open during active execution.

Minimum acceptable behavior:

- session detail reloads or refetches on `SessionEvent`, `CostUpdate`, or `Resync` for the viewed session

## 4. Cache Policy

### 4.1 Remove `/api/costs` from stale-while-revalidate

Files:

- `dashboard/src/service-worker.ts`

Required change:

- `/api/costs` must not be part of the stale-while-revalidate set.
- Use `networkFirstWithCache` only if the team explicitly wants offline read fallback for authenticated users.
- Prefer no cache for cost operational state if there is any ambiguity.

Design rule:

- Cost correctness cannot depend on an incidental Authorization-header bypass.

## 5. Tests and Gates

### 5.1 Backend tests

Add or update tests for:

- same-day multi-run cap enforcement
- UTC day rollover reset
- restart restore on same UTC day
- session `cumulative_cost` independence from pagination window
- `CostUpdate` emission on non-compaction spend
- `CostUpdate` emission on compaction spend

Likely files:

- `crates/ghost-gateway/tests/gateway_tests.rs`
- dedicated new integration tests under `crates/ghost-gateway/tests/`

### 5.2 SDK tests

Add or update tests for:

- typed `client.costs.list()`
- websocket cost event typing
- generated-type parity if generation is part of normal workflow

Likely file:

- `packages/sdk/src/__tests__/client.test.ts`

### 5.3 Dashboard tests

Add or update tests for:

- `/costs` renders from the store
- websocket `CostUpdate` mutates visible totals without reload
- session detail cost display does not depend on event page size

Use existing dashboard test infrastructure where available.

## 6. Completion Checklist

The implementation is not complete until all of the following are true:

- no route-local cost fetch remains on `/costs`
- no duplicate dashboard `AgentCost` interface remains
- no heuristic session cost math remains in `api/sessions.rs`
- runner `daily_spend` is seeded from `CostTracker`
- a UTC rollover task exists in bootstrap/runtime
- websocket cost events are defined and consumed
- `/api/costs` is not on stale-while-revalidate policy
- tests prove the corrected semantics
