# Costs Remediation Tasks

Status: March 11, 2026

Authoritative design: `COSTS_MASTER_REMEDIATION_SPEC.md`

Implementation companion: `COSTS_REMEDIATION_IMPLEMENTATION.md`

Purpose: provide the dependency-ordered execution tracker for the ADE costs remediation program.

## Execution Rules

- Do not start dashboard refactors before backend semantics are corrected.
- Do not add websocket cost events without typing them in the SDK.
- Do not close any task until tests for that layer exist or are updated.
- Do not treat manual page reload as an acceptable freshness strategy.

## Phase 1. Backend Truth

### T1. Seed runner daily spend from the gateway ledger

Files:

- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-agent-loop/src/runner.rs`

Done when:

- runner instances begin with same-day spend from `CostTracker`
- cap checks fail correctly when prior same-day spend already exceeds cap
- regression test covers the scenario

### T2. Add UTC day rollover task

Files:

- `crates/ghost-gateway/src/cost/tracker.rs`
- `crates/ghost-gateway/src/bootstrap.rs`

Done when:

- daily totals reset automatically after UTC day change
- rollover is explicit and observable
- regression test covers rollover behavior

### T3. Replace session cost heuristic with session ledger total

Files:

- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/cost/tracker.rs`

Done when:

- `cumulative_cost` is stable across different `offset` and `limit` values
- no hard-coded per-token cost heuristic remains in session events API
- regression test covers paginated session reads

## Phase 2. Contract and Event Propagation

### T4. Add websocket cost domain events

Files:

- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`

Done when:

- cost mutations emit typed domain events
- day rollover emits typed reset event
- backend test proves event emission

### T5. Type cost websocket events in the SDK

Files:

- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/index.ts`
- related SDK tests

Done when:

- dashboard code can consume cost events without local event casts
- SDK tests cover new event variants

## Phase 3. Dashboard Unification

### T6. Remove duplicate dashboard cost contract

Files:

- `dashboard/src/lib/stores/costs.svelte.ts`

Done when:

- the store imports `AgentCostInfo` from `@ghost/sdk`
- no local `AgentCost` interface remains

### T7. Make the shared costs store live

Files:

- `dashboard/src/lib/stores/costs.svelte.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`

Done when:

- the store responds to `CostUpdate`
- the store responds to `CostDailyReset`
- the store refreshes on `Resync`

### T8. Move `/costs` onto the shared store

Files:

- `dashboard/src/routes/costs/+page.svelte`

Done when:

- the route no longer owns its own fetch lifecycle
- visible totals update after websocket cost events without reload

### T9. Align agent detail cost view

Files:

- `dashboard/src/routes/agents/[id]/+page.svelte`

Done when:

- agent detail reflects the same store-backed or same-refresh-path totals as `/costs`
- no divergent per-page cost semantics remain

### T10. Align session detail cost view

Files:

- `dashboard/src/routes/sessions/[id]/+page.svelte`

Done when:

- session detail renders authoritative session total
- refresh behavior exists for live sessions or is explicitly documented as static read-on-open

## Phase 4. Cache Policy

### T11. Remove `/api/costs` from stale-while-revalidate

Files:

- `dashboard/src/service-worker.ts`

Done when:

- cost endpoint is no longer treated as SWR content
- policy is explicit rather than relying on auth-header non-cacheability

## Phase 5. Verification

### T12. Backend verification

Run or update:

- relevant Rust tests for gateway cost tracking and runtime safety

Done when:

- cap enforcement
- rollover
- session cost truth
- event propagation

are all under test

### T13. SDK verification

Run or update:

- SDK client and websocket tests

Done when:

- new cost events and cost API shapes are covered

### T14. Dashboard verification

Run or update:

- dashboard tests for the costs route and session/agent cost rendering

Done when:

- the dedicated costs surface updates live
- the agent detail and session detail surfaces match corrected backend semantics

## Exit Criteria

The program is complete only when:

- T1 through T14 are complete
- the codebase has one cost ledger, one live dashboard store, and one set of semantics
- no known cost-surface drift remains between backend truth, runtime gates, SDK types, and ADE UI
