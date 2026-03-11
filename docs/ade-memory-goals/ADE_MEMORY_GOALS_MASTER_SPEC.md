# ADE Memory + Goals Master Spec

Status: March 11, 2026

Purpose: define the authoritative remediation target for the ADE memory and goals system across storage, runtime, gateway APIs, SDK, and dashboard.

This spec is based on the live codebase as inspected on March 11, 2026. It is not derived from older aspirational architecture documents.

## Standard

This work is held to the following bar:

- No proposal surface without a durable state application path.
- No dashboard filter whose values do not match the stored contract.
- No runtime hydration path that reads state the UI cannot inspect.
- No UI state whose source of truth is different from the runtime source of truth.
- No approval action that mutates proposal metadata without mutating the intended domain state.
- No public SDK wrapper that omits live gateway capabilities required for the ADE workflow.
- No memory or goal view without deep-linkable item detail.
- No green release without end-to-end tests for create, approve, materialize, fetch, search, and hydrate.

## Scope

This spec covers:

- durable memory persistence and retrieval
- goal proposal persistence, decisioning, and materialization
- runtime hydration from durable memory and goal state
- memory and goals REST contracts
- memory and goals SDK wrappers
- memory and goals dashboard surfaces
- websocket or refresh semantics needed to keep those surfaces live
- test gates required to prevent recurrence

This spec does not redesign unrelated ADE areas such as channels, workflows, studio streaming, or autonomy control plane behavior except where those systems consume memory or goals.

## Confirmed Current-State Gaps

### G1. Proposals are not enough.

The current system persists proposals, but approved `GoalChange` and `MemoryWrite` do not have a complete domain-state materialization path. That leaves the ADE with proposal history but incomplete durable truth.

### G2. The goals tab currently mixes two concepts.

The UI is effectively treating “goal proposals” as “goals”. These are not the same domain object.

Target correction:

- goals are durable domain state
- proposals are change intents and decision history

### G3. The memory tab is contract-drifted.

The dashboard sends filter values that do not match the serialized memory enums and importance levels stored in the database.

### G4. ADE drilldown is broken.

A proposal can cite memories, the gateway can fetch a specific memory, but the dashboard does not expose a memory detail route and does not link cited memories to an item detail surface.

### G5. The runtime/UI truth boundary is weak.

Runtime hydration expects durable memories, goals, and reflections that the UI cannot currently manage with full fidelity.

## Target End State

The ADE memory and goals system must behave as a single coherent unit with the following properties.

### P1. Canonical domain model

There are three distinct first-class concepts:

- durable memory
- durable goal state
- proposal history

They must not be conflated in storage, APIs, SDK types, or UI labeling.

### P2. Proposal application semantics

Every proposal decision falls into one of these categories:

- no-op rejection
- durable materialization
- durable state mutation
- durable supersession

If a proposal is approved or auto-applied, the system must atomically write the resulting durable state change before the decision is reported as complete.

### P3. Durable goal truth

The ADE must expose active goal state directly, not reconstruct it opportunistically from unresolved proposal rows.

Minimum required goal truth:

- goal identity
- lineage
- current revision
- approval state
- source proposal
- created_at
- updated_at
- superseded_by or archived_at when applicable

### P4. Durable memory truth

The memory system must provide:

- durable list view
- deep detail view
- full-text search
- type and importance filtering using canonical enums
- archived-state lifecycle
- graph view
- references from proposals to memories

### P5. Runtime alignment

The runtime hydrator must read from the same durable goal and memory truth the ADE displays.

No hidden goal layer.
No hidden reflection layer.
No synthetic UI-only shape that diverges from runtime memory shape.

### P6. Live UI semantics

The dashboard must update after:

- proposal decision
- proposal materialization
- memory write
- memory archive
- memory unarchive
- websocket resync

### P7. Contract discipline

The gateway OpenAPI, generated SDK types, hand-written SDK wrappers, and dashboard consumers must agree on:

- status vocabulary
- enum casing
- payload shape
- pagination
- item detail routes

## Required User-Facing Surfaces

### Goals Tab

The goals tab must show:

- active goals
- pending proposals
- recently resolved proposals
- per-goal lineage and revision metadata
- proposal detail with decision history
- links to cited memories

The default top-level mental model is:

- “Goals” means active durable goals
- “Proposals” means requested changes to those goals or to memory

### Memory Tab

The memory tab must show:

- list view
- detail view
- graph view
- search
- canonical filters
- archive/unarchive controls
- provenance references where available

## Required Engineering Invariants

- A successful approval response must mean the resulting durable state already exists.
- A durable goal row must reference the proposal that created or last mutated it.
- A memory event and latest snapshot for a materialized memory change must be persisted in the same transaction.
- Goal list semantics must derive from domain state, not unresolved legacy decision rows.
- Filter enums must be generated from canonical types or normalized centrally.
- Every proposal-to-memory reference displayed in the UI must be navigable.
- Dashboard item detail views must resolve by durable identifier, not list re-query hacks.

## Non-Negotiable Exit Criteria

This work is not done until all of the following are true:

- approving a goal proposal changes durable goal state
- approving a memory-write proposal changes durable memory state
- runtime hydration consumes the materialized goal and memory state
- the goals tab no longer uses unresolved proposal rows as its primary definition of “pending goals”
- the memory tab filters produce correct results for canonical stored values
- the dashboard exposes memory detail drilldown
- the SDK exposes the gateway memory lifecycle actions needed by the dashboard
- end-to-end tests cover proposal creation, approval, materialization, retrieval, and hydration
