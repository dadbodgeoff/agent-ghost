# ADE Memory + Goals Architecture Contract

Purpose: define the target architecture and ownership boundaries for ADE memory and goals.

## Canonical Domain Objects

### 1. Proposal

Definition:

- an intent to create or mutate durable state
- append-only decision history
- never the canonical durable state itself

Minimum responsibilities:

- capture proposer, operation, target type, content, cited memory ids, lineage metadata, and transitions
- preserve full audit trail for approval and rejection

### 2. Durable Goal

Definition:

- the active, operator-visible goal state for an agent

Minimum fields:

- `goal_id`
- `agent_id`
- `lineage_id`
- `subject_key`
- `revision`
- `status`
- `content`
- `source_proposal_id`
- `created_at`
- `updated_at`

### 3. Durable Memory

Definition:

- the operator-visible and runtime-visible durable memory state for an agent

Minimum fields:

- `memory_id`
- `agent_id`
- latest snapshot
- event history
- archived state
- memory type
- importance
- confidence
- tags
- created_at
- updated_at

## Ownership by Layer

### Agent Loop

Owns:

- extracting proposals from model output
- routing proposals through decision logic
- invoking proposal materialization for auto-approved decisions

Must not own:

- ad hoc direct writes that bypass canonical materialization

### Gateway API

Owns:

- human approval and rejection
- transactional application of proposal decisions
- item retrieval and list semantics
- websocket event emission for UI refresh

Must not own:

- alternate shadow logic for goal truth that the runtime does not use

### Storage Layer

Owns:

- append-only proposal history
- append-only memory event log
- latest memory snapshot retrieval
- durable goal-state projection
- atomic transactions and referential integrity

### SDK

Owns:

- exact client wrappers over live gateway contracts
- canonical enum and status vocabulary for dashboard consumers

Must not own:

- invented status aliases
- dashboard-only filter values

### Dashboard

Owns:

- rendering
- user interaction
- routing
- live refresh orchestration

Must not own:

- custom reinterpretation of proposal or memory state

## Required Flows

### Flow A. Auto-Approved Memory Write

1. Agent emits proposal.
2. Agent loop extracts proposal.
3. Proposal is persisted.
4. Auto-approval decision is made.
5. Materializer writes:
   - memory event
   - latest memory snapshot
   - any required audit row
   - proposal transition row
6. Gateway or loop emits refresh event.
7. Runtime hydration and dashboard list/search/detail all see the same new memory.

### Flow B. Human-Approved Goal Change

1. Proposal exists in pending review.
2. Human approves through gateway.
3. Approval transaction writes:
   - proposal transition
   - durable goal-state row update or insert
   - lineage-head update
   - audit row
4. Gateway emits decision and state-changed events.
5. Goals tab refreshes active goals and proposal history.
6. Runtime hydration reads the new durable goal state.

### Flow C. Proposal-to-Memory Drilldown

1. Proposal detail view renders cited memory ids.
2. Each cited id links to `/memory/[id]`.
3. Memory detail route loads exact durable item from `/api/memory/:id`.
4. User can navigate back to goal proposal context without losing state.

## Required Data Truth Rules

### Goals

The goals tab must be backed by durable goal state plus proposal history.

It must not rely on:

- `resolved_at IS NULL` as the sole meaning of “pending”
- legacy decision strings as the sole meaning of “approved”
- unresolved proposal rows as the only representation of active goals

### Memories

The memory tab must be backed by latest durable snapshots and event-linked ownership.

It must not rely on:

- enum values invented in the UI
- a list page with no detail page
- hidden archive endpoints not surfaced in the SDK

## Required API Surface

The target API surface must include:

- `GET /api/goals`
  - active durable goals with status filters
- `GET /api/goals/proposals`
  - proposal history and pending queue
- `GET /api/goals/proposals/:id`
  - proposal detail
- `POST /api/goals/proposals/:id/approve`
- `POST /api/goals/proposals/:id/reject`
- `GET /api/memory`
- `GET /api/memory/:id`
- `GET /api/memory/search`
- `GET /api/memory/graph`
- `POST /api/memory/:id/archive`
- `POST /api/memory/:id/unarchive`

If the team prefers to keep the current route names, the behavior still must meet this contract. Naming is negotiable. Semantics are not.

## Required UI Surface

### Goals

- active-goal list
- pending-proposal queue
- resolved-proposal history
- goal detail or side panel
- proposal detail route

### Memory

- list route
- detail route
- graph route
- filter/search controls using canonical generated enums
- archive/unarchive actions

## Eventing Contract

At minimum, the dashboard must refresh goals or memory after:

- proposal decision
- proposal materialization
- memory archive
- memory unarchive
- websocket resync

If explicit websocket event types are introduced for memory and goal-state mutation, they should become the preferred path. If not, resync handlers must still guarantee correctness.
