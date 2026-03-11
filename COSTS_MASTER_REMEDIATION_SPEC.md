# Costs Master Remediation Spec

Status: March 11, 2026

Purpose: define the authoritative remediation plan for ADE cost tracking, spending-cap enforcement, session cost accounting, and all dashboard surfaces that expose cost state.

This document is based on the live code, not on historical intent. If this spec conflicts with ad hoc comments, tests, or UI assumptions, this spec wins.

## Standard

This work is held to the following bar:

- No cost number without one explicit source of truth.
- No enforcement path that uses weaker data than the operator UI.
- No "daily" total without an explicit day-boundary policy.
- No session cost derived from paginated event subsets.
- No duplicated frontend cost model when the SDK contract already exists.
- No live ADE surface that silently goes stale while the underlying state changes.
- No public contract or websocket event added without type ownership in backend, generated types, SDK, and dashboard consumers.

## Scope

This spec covers:

- backend cost tracking and persistence
- spending-cap enforcement in the live runtime
- autonomy budget checks
- session cost reporting
- cost websocket propagation
- `/api/costs`
- `/api/sessions/:id/events` cost semantics
- dashboard costs state management
- `/costs`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`
- service-worker cache policy for cost surfaces
- the tests and gates required to prevent recurrence

This spec does not require:

- new external billing providers
- historical multi-day reporting
- finance exports
- tenant billing
- changing the existing `AgentCostInfo` response shape unless required for correctness

## Primary Sources

- `crates/ghost-gateway/src/cost/tracker.rs`
- `crates/ghost-gateway/src/cost/spending_cap.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/api/costs.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/autonomy.rs`
- `crates/ghost-gateway/src/shutdown.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/context/run_context.rs`
- `crates/ghost-llm/src/cost.rs`
- `dashboard/src/routes/costs/+page.svelte`
- `dashboard/src/lib/stores/costs.svelte.ts`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/service-worker.ts`
- `packages/sdk/src/costs.ts`
- `packages/sdk/src/generated-types.ts`

## Confirmed Findings

### F1. Spending-cap enforcement is not reading the authoritative daily total.

The gateway sets `runner.spending_cap` but does not seed `runner.daily_spend` from the gateway `CostTracker`. The runner therefore begins each run as though earlier same-day spend does not exist.

Implication:

- cap enforcement can allow a run that should already be blocked
- the UI can show "over budget" while the runtime still proceeds
- the ledger and the gate are not the same system

Evidence:

- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-agent-loop/src/runner.rs`

### F2. "Daily" cost totals do not have a runtime day-boundary mechanism.

`CostTracker::reset_daily()` exists, but no background task calls it. The only recurring cost task persists state every five minutes.

Implication:

- daily totals can become process-lifetime totals
- autonomy budget checks can drift
- cost pages can present incorrect "remaining" amounts

Evidence:

- `crates/ghost-gateway/src/cost/tracker.rs`
- `crates/ghost-gateway/src/bootstrap.rs`

### F3. Session cost reporting is using an invented pricing formula instead of the tracked ledger.

`GET /api/sessions/:id/events` computes `cumulative_cost` from `token_count * 0.000003` across the returned page of events. This is not the tracked session total and does not use actual provider pricing.

Implication:

- the same session can show different cumulative cost at different pagination offsets
- session cost can disagree with actual tracked spend
- operators cannot trust session detail as financial truth

Evidence:

- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/cost/tracker.rs`
- `crates/ghost-llm/src/cost.rs`

### F4. The Costs page is not wired as a live ADE surface.

`dashboard/src/routes/costs/+page.svelte` fetches once on mount and owns local state. A separate `costsStore` exists, but the page does not use it. No cost-specific websocket event exists.

Implication:

- the dedicated costs route becomes stale during active execution
- there are two dashboard implementations for the same domain
- future fixes can land in one path and miss the other

Evidence:

- `dashboard/src/routes/costs/+page.svelte`
- `dashboard/src/lib/stores/costs.svelte.ts`
- `crates/ghost-gateway/src/api/websocket.rs`

### F5. ADE cost surfaces do not share one semantic contract.

The global Costs page, the per-agent page, and the session page do not all read from the same backend truth with the same freshness policy.

Implication:

- operators can see three different answers to "what did this cost?"
- debugging cost incidents becomes guesswork
- alerts, pauses, and UI displays can diverge

Evidence:

- `dashboard/src/routes/costs/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`
- `crates/ghost-gateway/src/api/costs.rs`
- `crates/ghost-gateway/src/api/sessions.rs`

### F6. The dashboard keeps a duplicate frontend cost type.

The dashboard store defines its own `AgentCost` interface even though the SDK already exports `AgentCostInfo`.

Implication:

- backend and frontend contracts can drift without type failure
- fields can be forgotten in one layer
- the store is not contract-owned by the SDK

Evidence:

- `dashboard/src/lib/stores/costs.svelte.ts`
- `packages/sdk/src/costs.ts`

### F7. Cost freshness and cache policy are under-specified.

The service worker categorizes `/api/costs` as stale-while-revalidate, while the route itself is intended as live operational state. Today most authenticated requests avoid caching because of the Authorization-header guard, but the policy is still wrong by design and fragile to future changes.

Implication:

- live financial state relies on incidental auth behavior instead of explicit policy
- future tokenless flows could reintroduce stale cost reads
- the code advertises a cache policy that should not exist

Evidence:

- `dashboard/src/service-worker.ts`
- `packages/sdk/src/client.ts`

## Target End State

The ADE cost system must satisfy all of the following:

1. `CostTracker` is the sole authority for in-process agent daily totals, session totals, and compaction totals.
2. Every runtime spending-cap decision reads from the same `CostTracker` totals that power `/api/costs`.
3. Daily totals reset on an explicit UTC day boundary, with persistence and restore semantics that preserve truth across restart.
4. Session detail cost is sourced from the authoritative session ledger, never from paginated heuristics.
5. The backend emits typed websocket events for cost mutations and daily reset boundaries.
6. The dashboard uses one shared costs state path backed by SDK types.
7. `/costs`, agent detail, and session detail become consistent views over the same cost domain, not separate implementations.
8. Cache policy for cost endpoints is explicit and conservative.

## Architecture Decisions

### D1. Keep `CostTracker` as the domain ledger, not the session API.

The tracker already records:

- per-agent daily totals
- per-session totals
- per-agent compaction totals

The remediation must strengthen and expose that ledger instead of creating a second accounting path in `api/sessions.rs`.

### D2. Daily cost semantics are UTC, not local-time inferred.

The current tracker already uses `chrono::Utc` for persisted snapshot dates. The system will formalize that:

- daily totals are UTC-day totals
- rollover occurs when the UTC date changes
- UI text may say "UTC day" or "daily" only if the UTC basis is documented in operator-facing hints

### D3. Session cost must be full-session truth regardless of pagination.

`SessionEventsResponse.cumulative_cost` remains allowed, but its meaning changes from "sum over returned rows" to "tracked full-session total."

That value must:

- not change when `offset` changes
- not depend on the number of rows returned
- match the session ledger in `CostTracker`

### D4. Cost changes require explicit websocket contract support.

The websocket contract must gain cost-domain events. At minimum:

- `CostUpdate`
- `CostDailyReset`

`CostUpdate` must carry enough fields for the dashboard to update visible views without re-deriving ledger semantics from unrelated endpoints.

### D5. The dashboard cost model is SDK-owned.

The dashboard may add local view helpers, but not a duplicate domain interface. `AgentCostInfo` from `@ghost/sdk` is the owner of the REST shape.

### D6. `/api/costs` is operational state, not a cache-friendly content feed.

The service worker must not treat the costs endpoint as stale-while-revalidate. For authenticated operational dashboards, correctness wins over speculative caching.

## Required Contracts

### REST: `GET /api/costs`

The existing response fields remain the baseline contract:

- `agent_id`
- `agent_name`
- `daily_total`
- `compaction_cost`
- `spending_cap`
- `cap_remaining`
- `cap_utilization_pct`

Semantics:

- `daily_total` is the authoritative UTC-day total from `CostTracker`
- `compaction_cost` is the authoritative UTC-day compaction subtotal from `CostTracker`
- `cap_remaining` is derived from the same `daily_total`
- `cap_utilization_pct` is derived from the same `daily_total`

### REST: `GET /api/sessions/:id/events`

`cumulative_cost` remains in the payload but must mean:

- the authoritative full-session total from `CostTracker`
- not a heuristic
- not a paginated subtotal

### WebSocket

Add:

- `CostUpdate { agent_id, session_id, daily_total, session_total, compaction_cost, spending_cap, cap_remaining, cap_utilization_pct, is_compaction }`
- `CostDailyReset { reset_date_utc }`

The exact field names may vary, but the event must be self-sufficient for dashboard reconciliation.

## Acceptance Bar

The remediation is complete only when all of the following are true:

- a run that has already exceeded same-day spend is blocked before execution starts
- a UTC day rollover clears in-memory daily totals and keeps restore semantics correct after restart
- session detail shows the same session total regardless of pagination parameters
- `/costs` updates without manual reload during active spend changes
- agent detail and global costs view show the same daily totals for the same agent
- dashboard code no longer defines a duplicate `AgentCost` contract
- websocket cost events are typed in backend, generated artifacts, SDK, and dashboard consumers
- tests cover the critical failure modes that produced the current drift
