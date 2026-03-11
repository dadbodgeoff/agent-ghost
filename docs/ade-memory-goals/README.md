# ADE Memory + Goals Documentation Pack

Status: March 11, 2026

Purpose: provide a build-ready document package for remediating the ADE memory and goals surfaces from storage through runtime, API, SDK, and dashboard.

This package is written for an implementation agent. The documents are ordered. Read them in sequence.

## Consumption Order

1. `ADE_MEMORY_GOALS_MASTER_SPEC.md`
   - authoritative product and engineering target
   - defines the invariants and exit criteria
2. `ADE_MEMORY_GOALS_ARCHITECTURE_CONTRACT.md`
   - target system model
   - defines ownership boundaries and end-to-end flows
3. `ADE_MEMORY_GOALS_IMPLEMENTATION_PLAN.md`
   - execution order
   - exact workstreams, file targets, and dependency order
4. `ADE_MEMORY_GOALS_VALIDATION_PLAN.md`
   - required tests, audits, rollout checks, and failure gates
5. `ADE_MEMORY_GOALS_AGENT_HANDOFF.md`
   - the single handoff brief that can be given to an implementation agent

## Package Rules

- If any older ADE memory/goals doc conflicts with this package, this package wins.
- The master spec is the authority for target behavior.
- The architecture contract is the authority for domain boundaries and data flow.
- The implementation plan is the authority for task ordering.
- The validation plan is the authority for done-ness.
- The handoff brief must not be used without the other four documents.

## Current Reality This Package Addresses

- proposals are persisted, but approved goal and memory proposals are not fully materialized into durable ADE state
- goal status surfaces are inconsistent for auto-decisions
- memory filtering is contract-incompatible with stored enum values
- proposal-to-memory drilldown is broken in the dashboard
- memory lifecycle actions exist in the gateway but are not fully surfaced in the SDK/dashboard
- coverage is not currently strong enough to prevent regression across these surfaces

## Final Deliverable

The final deliverable of this package is not another planning artifact. It is a cohesive ADE where:

- goals are durable state, not just proposal history
- memory is searchable, inspectable, linkable, archivable, and live-refreshing
- runtime hydration uses the same durable truth the UI shows
- proposal approval changes real state atomically and audibly
- SDK and dashboard behavior are generated from the same contract the gateway serves
