# Costs Agent Handoff

You are implementing the ADE costs remediation program defined in:

1. `COSTS_MASTER_REMEDIATION_SPEC.md`
2. `COSTS_REMEDIATION_IMPLEMENTATION.md`
3. `COSTS_REMEDIATION_TASKS.md`

Read them in that order before changing code.

## Mission

Make ADE cost tracking, cost display, and spending-cap enforcement operate as one coherent system across backend, SDK, and dashboard.

You are not polishing a page. You are repairing a domain.

## Non-Negotiable Rules

- Do not introduce a second cost ledger.
- Do not leave any UI surface on heuristic cost math.
- Do not leave `/costs` as a fetch-once page.
- Do not keep a duplicate dashboard `AgentCost` interface.
- Do not ship websocket cost events without SDK typing.
- Do not claim completion without tests in the touched layers.

## Required Outcomes

1. Runtime spending-cap enforcement reads same-day totals from `CostTracker` before execution.
2. UTC day rollover is explicit and automatic.
3. Session `cumulative_cost` is ledger truth, not paginated approximation.
4. Backend emits typed cost websocket events.
5. Dashboard costs state is centralized and live.
6. `/costs`, agent detail, and session detail become semantically aligned.
7. `/api/costs` is removed from stale-while-revalidate policy.

## Implementation Order

1. Backend truth
2. Backend event propagation
3. SDK typing
4. Dashboard store unification
5. Dashboard route integration
6. Cache-policy cleanup
7. Tests and verification

## Files You Are Expected To Touch

- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-gateway/src/cost/tracker.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/api/costs.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `dashboard/src/lib/stores/costs.svelte.ts`
- `dashboard/src/routes/costs/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`
- `dashboard/src/service-worker.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/costs.ts`
- tests in gateway, SDK, and dashboard layers

## Definition of Done

The work is done only if all of the following are true:

- an agent that already spent over cap earlier in the UTC day is blocked on the next run
- cost totals roll over automatically on UTC day boundary
- session cost does not change when session events are paginated differently
- the global costs page updates live without reload
- agent detail and global costs page agree for the same agent
- dashboard code no longer defines a duplicate cost domain type
- new websocket cost events are typed end to end
- relevant tests pass or any blocked test gap is explicitly documented with a precise reason

## Delivery Expectations

When you finish:

1. summarize the architecture changes, not just edited files
2. list the tests you ran
3. list any residual risks
4. if anything remains incomplete, say exactly what and why

No shortcuts. No "follow-up recommended" for critical-path items that belong in this remediation.
