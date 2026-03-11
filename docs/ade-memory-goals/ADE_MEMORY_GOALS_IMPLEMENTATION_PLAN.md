# ADE Memory + Goals Implementation Plan

Purpose: define the execution order to build the target state without hand-waving.

## Phase 1. Establish Canonical State Boundaries

### 1.1 Separate “goals” from “goal proposals”

Deliverable:

- explicit durable goal-state projection in storage and API

Primary files:

- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- `crates/cortex/cortex-storage/src/queries/mod.rs`
- `crates/cortex/cortex-storage/src/migrations/`
- `crates/ghost-gateway/src/api/goals.rs`

Tasks:

- add or formalize a durable goal-state table or projection query
- define goal-state status vocabulary
- stop using unresolved proposal rows as the primary meaning of active goals
- preserve proposal history as a separate surface

### 1.2 Define proposal materialization entrypoints

Deliverable:

- one canonical function for auto-approval application
- one canonical function for human-approval application

Primary files:

- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-gateway/src/api/goals.rs`
- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- `crates/cortex/cortex-storage/src/queries/memory_event_queries.rs`
- `crates/cortex/cortex-storage/src/queries/memory_snapshot_queries.rs`

Tasks:

- implement a materializer for approved `GoalChange`
- implement a materializer for approved `MemoryWrite`
- keep `ReflectionWrite` on the same materialization path rather than as a special orphan branch
- require materialization before returning “approved”

## Phase 2. Fix Storage and Runtime Alignment

### 2.1 Materialize durable goal state

Deliverable:

- runtime hydration can load active goals from durable goal state without reconstructing from proposal history

Primary files:

- `crates/ghost-gateway/src/api/runtime_execution.rs`
- `crates/read-only-pipeline/src/assembler.rs`
- `crates/read-only-pipeline/src/snapshot.rs`

Tasks:

- update runtime hydration to read canonical goal state
- keep reflections aligned with the same durable source
- remove or reduce hidden fallback paths that diverge from UI truth

### 2.2 Materialize durable memory state consistently

Deliverable:

- approved memory-write proposals create both event and snapshot rows atomically

Primary files:

- `crates/cortex/cortex-storage/src/queries/memory_event_queries.rs`
- `crates/cortex/cortex-storage/src/queries/memory_snapshot_queries.rs`
- `crates/ghost-gateway/src/api/memory.rs`
- `crates/ghost-agent-loop/src/tools/builtin/memory.rs`

Tasks:

- ensure every approved memory mutation writes event plus snapshot
- ensure actor ownership is attached in a way that list/search/filter can use
- preserve archival and audit semantics

## Phase 3. Correct Gateway Contract Semantics

### 3.1 Goals API split or semantic correction

Deliverable:

- goal list endpoint means active goals
- proposal list endpoint means proposal history

Primary files:

- `crates/ghost-gateway/src/api/goals.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/goals.ts`
- `packages/sdk/src/generated-types.ts`

Tasks:

- normalize status filters around canonical state
- include auto-applied outcomes in correct filters
- expose active goal state separately from proposal queue if needed
- regenerate types and remove wrapper drift

### 3.2 Memory API completion

Deliverable:

- SDK and dashboard can use write/archive/unarchive lifecycle operations

Primary files:

- `crates/ghost-gateway/src/api/memory.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/memory.ts`

Tasks:

- make sure write/archive/unarchive are fully documented in OpenAPI
- add SDK wrappers for lifecycle actions
- keep request and response payloads typed from generated contract

## Phase 4. Repair Dashboard Information Architecture

### 4.1 Goals tab redesign

Deliverable:

- a true goals surface, not a proposal-only queue

Primary files:

- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/goals/[id]/+page.svelte`
- `dashboard/src/components/GoalCard.svelte`

Tasks:

- render active goals and pending proposals separately
- make status tabs reflect canonical state
- show lineage and revision metadata
- keep proposal detail route focused on proposal history

### 4.2 Memory detail route

Deliverable:

- `/memory/[id]` route with durable memory detail

Primary files:

- `dashboard/src/routes/memory/+page.svelte`
- `dashboard/src/routes/memory/[id]/+page.svelte` (new)
- `dashboard/src/routes/memory/graph/+page.svelte`
- `dashboard/src/components/MemoryCard.svelte`

Tasks:

- add item detail route
- link memory cards and cited memories to detail
- show snapshot, metadata, archive state, and related references

### 4.3 Canonical memory filters

Deliverable:

- filter controls generated from canonical enum values

Primary files:

- `dashboard/src/routes/memory/+page.svelte`
- `packages/sdk/src/memory.ts`

Tasks:

- remove invalid filter options
- normalize or generate memory type options from contract
- normalize importance values to canonical enum names
- add confidence controls if the backend supports them

### 4.4 Live refresh wiring

Deliverable:

- memory and goals surfaces refresh correctly after mutation and resync

Primary files:

- `dashboard/src/lib/stores/memory.svelte.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/routes/memory/+page.svelte`
- `dashboard/src/routes/goals/+page.svelte`

Tasks:

- wire memory page to store or websocket resync
- emit or consume state-change events for goals and memory
- make route-level data refresh deterministic after approval and lifecycle actions

## Phase 5. Validation and Cutover

### 5.1 Contract regeneration and parity

Deliverable:

- no drift between gateway, OpenAPI, generated types, SDK wrappers, and dashboard consumers

Primary files:

- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/__tests__/client.test.ts`
- `scripts/check_openapi_parity.py`

### 5.2 End-to-end tests

Deliverable:

- hard gates covering proposal creation, approval, materialization, retrieval, and hydration

Primary files:

- `crates/ghost-gateway/tests/`
- `dashboard/tests/`

## Execution Rules

- Do not start dashboard redesign before the canonical state semantics are fixed.
- Do not ship a new goals UI on top of legacy unresolved-proposal semantics.
- Do not ship new memory filters before the enum contract is canonicalized.
- Do not call the work complete until runtime hydration and dashboard views agree on the same durable truth.
