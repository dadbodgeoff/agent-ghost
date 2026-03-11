# ADE Memory + Goals Agent Handoff

Purpose: single execution brief for the implementation agent.

## Mission

Build the ADE memory and goals system into a cohesive end-to-end unit from storage through runtime, gateway, SDK, and dashboard.

The target is not “make the pages look better”.

The target is:

- approved proposals materialize real durable state
- runtime hydration uses that same durable state
- goals and proposals are separate concepts in both API and UI
- memory is fully navigable and lifecycle-managed in the ADE
- filters, statuses, and enums are canonical across all layers

## Required Reading

Read these documents before making changes:

1. `docs/ade-memory-goals/ADE_MEMORY_GOALS_MASTER_SPEC.md`
2. `docs/ade-memory-goals/ADE_MEMORY_GOALS_ARCHITECTURE_CONTRACT.md`
3. `docs/ade-memory-goals/ADE_MEMORY_GOALS_IMPLEMENTATION_PLAN.md`
4. `docs/ade-memory-goals/ADE_MEMORY_GOALS_VALIDATION_PLAN.md`

## Operating Rules

- Do not treat proposal rows as active goal truth.
- Do not add new UI filter values that are not canonical generated contract values.
- Do not return approval success before the durable state mutation is committed.
- Do not keep special-case reflection persistence as a separate pattern if the proposal materializer can own it.
- Do not introduce hand-written SDK enum forks.
- Do not leave memory detail inaccessible from a cited-memory reference.

## Build Order

1. Canonicalize storage and domain semantics.
2. Implement proposal materialization for approved goal and memory mutations.
3. Align runtime hydration to the durable truth.
4. Fix and regenerate gateway and SDK contracts.
5. Rebuild dashboard goals and memory surfaces on top of those contracts.
6. Add tests and prove cutover.

## Expected Workstreams

### Workstream A. State semantics

- define durable goal-state truth
- preserve proposal history separately
- normalize status vocabulary

### Workstream B. Materialization

- auto-approved proposals apply durable mutations
- human-approved proposals apply durable mutations
- memory writes create event + snapshot atomically

### Workstream C. Runtime alignment

- hydration reads durable goal and memory state
- no hidden goal layer outside ADE visibility

### Workstream D. Contract repair

- gateway OpenAPI corrected
- generated SDK types regenerated
- hand-written wrappers aligned

### Workstream E. Dashboard repair

- goals tab shows active goals and proposals separately
- memory detail route added
- cited-memory drilldown works
- archive/unarchive surfaced
- live refresh wired

### Workstream F. Validation

- gateway tests
- SDK tests
- dashboard tests
- manual cutover checklist

## Minimum Done Definition

The work is only done when all of these are true:

- approving a goal proposal changes durable goal state
- approving a memory-write proposal changes durable memory state
- active goals in the dashboard match runtime hydration
- memory detail is reachable from proposal detail and memory list
- memory filters work with canonical stored values
- auto-applied proposals are classified correctly
- SDK covers the live lifecycle actions used by the dashboard
- automated tests exist for the critical flows

## Final Deliverable Format

When implementation is complete, produce a concise closeout containing:

- what changed
- which migrations were added
- which routes changed
- which SDK surfaces changed
- which dashboard routes were added or rewritten
- which tests now protect the behavior
- any residual risks or deferred items
