# Orchestration Architecture And Contracts

Status: March 11, 2026
Purpose: define the target architecture, contracts, and ownership model for orchestration.

## System Model

The orchestration subsystem is a vertical slice spanning:

- persistence
- gateway read models
- websocket event model
- SDK typed clients
- dashboard orchestration store
- route and render components

The system must be built around one rule: backend read models are authoritative, frontend state is derived, and components do not invent parallel truth.

## Target Ownership Model

### Persistence Ownership

- `delegation_state` remains the canonical record of delegation transitions
- `goal_proposals` and `goal_proposal_transitions` remain the canonical proposal lifecycle records
- `a2a_tasks` remains the canonical local record of outbound A2A dispatch and reconciled task state
- `discovered_agents` remains the canonical cache of configured and discovered remote peers

### Read Model Ownership

Create one orchestration read-model layer in the gateway:

- trust read model
- consensus read model
- sybil read model
- A2A read model

These may begin inside `mesh_viz.rs` and `a2a.rs`, but the target shape should move derivation logic into dedicated functions or modules rather than inline route SQL.

### Frontend Ownership

Create one orchestration store in dashboard code as the single source of truth for:

- trust nodes
- trust edges
- consensus rounds
- delegations
- sybil metrics
- discovered peers
- A2A tasks
- loading/error/resync state per slice

The route consumes the store. Child components receive props and action callbacks.

## Target Contract Surface

## 1. Trust Graph Contract

### Response Shape

The route may keep the current high-level payload shape:

```json
{
  "nodes": [],
  "edges": []
}
```

but every field must become explicit and derivable.

### Trust Node Rules

- `id` = canonical agent identifier
- `name` = current display name from agent registry
- `activity` = explicit activity/trust-support metric with documented derivation
- `convergence_level` = latest convergence level if present, otherwise explicit fallback

### Trust Edge Rules

- edge source and target must correspond to canonical agent identifiers
- edge weight must come from one explicit derivation path:
  - direct delegation history
  - computed trust score
  - both, if separately labeled
- if direct delegation and trust score are different concepts, the contract must expose both rather than overloading one `trust_score`

### Required Backend Behavior

- no swallowed SQL preparation errors
- no schema references to non-existent columns
- no hidden state mapping such as `'active'` when storage uses named state machine values

## 2. Consensus Contract

### Response Shape

The route may keep:

```json
{
  "rounds": []
}
```

but `rounds` must map to the actual proposal lifecycle model.

### Consensus Round Rules

At minimum each round must define:

- `proposal_id`
- `status`
- `approvals`
- `rejections`
- `threshold`
- optional `state_source`
- optional `updated_at`

### Semantics

- `status` must map to canonical lifecycle state, not a UI-local interpretation
- `approvals` and `rejections` must count real records
- if ADE does not persist peer votes, the contract must stop pretending it does and instead represent actual review/transition state
- if the majority threshold is only a display heuristic, it must be labeled as such

### Required Backend Source

Use:

- `goal_proposal_transitions`
- `goal_lineage_heads`
- v2 proposal state

Do not build final orchestration truth from legacy convenience queries over `goal_proposals` alone.

## 3. Sybil And Delegation Contract

### Response Shape

Keep:

```json
{
  "delegations": [],
  "sybil_metrics": {}
}
```

### Delegation Rules

Each delegation row must explicitly represent whether it is:

- current state
- terminal state
- active state
- historical transition

If the response mixes state snapshots and transition history, the contract must say so. Prefer one of:

- return only current delegation heads
- return transitions with explicit transition metadata

### Sybil Metric Rules

Minimum required metrics:

- total delegations
- unique delegators
- computed max chain depth
- delegator fan-out distribution or max fan-out
- cycle count or cycle risk indicator
- concentration indicator for suspicious delegation clustering

### Required Semantics

- `max_chain_depth` must be computed from the delegation graph
- "active delegations" must be defined by real state mapping
- risk metrics must distinguish topology risk from normal system volume

## 4. A2A Discovery Contract

### Discovery Model

Discovery must separate four concepts:

- configured peer
- discovered peer
- reachable peer
- verified peer

These must not collapse into a single boolean.

### Required Peer Fields

- `name`
- `description`
- `endpoint_url`
- `capabilities`
- `version`
- `reachable`
- `verified`
- `trust_score`
- `source`
  - `configured`
  - `discovered`
  - `configured+discovered`

### Discovery Rules

- configured peers inserted during bootstrap must remain visible as configured even before probe success
- discovery runs must not erase configured intent
- unreachable peers must remain inspectable with last-known metadata

## 5. A2A Task Contract

### Task Lifecycle

The target lifecycle must be explicit. Preferred states:

- `pending_local_validation`
- `dispatching`
- `submitted`
- `working`
- `completed`
- `failed`
- `canceled`
- `timed_out`
- `recovery_required`

If the remote ecosystem cannot provide some of these, the system must document which transitions are gateway-observed versus inferred.

### Required Fields

- `task_id`
- `target_agent`
- `target_url`
- `method`
- `status`
- `created_at`
- `updated_at`
- `input`
- `output`
- `status_source`
  - `local_gateway`
  - `remote_agent`
  - `operator_override`

### Reconciliation Paths

At least one of the following must exist:

- remote polling
- callback/webhook ingestion
- SSE proxy/reconciliation
- explicit operator-driven refresh contract

Without one of these, the UI must not market itself as live task execution tracking.

## Websocket Contract Requirements

The orchestration store must react to:

- `AgentStateChange`
- `ProposalDecision`
- `ScoreUpdate`
- `A2ATaskUpdate`
- any future delegation-specific event
- `Resync`

If delegation-specific events do not yet exist, add them. Orchestration should not rely on unrelated events and hope that polling hides the gap.

## Dashboard State Architecture

## Store Shape

Create one store, for example:

- `dashboard/src/lib/stores/orchestration.svelte.ts`

The store should own:

- data slices
- stale/loading/error markers
- initial load
- targeted reload per slice
- websocket subscription lifecycle
- resync behavior

## Route Responsibilities

The route should:

- initialize the store
- bind tabs and local presentation state
- pass render data and actions down

The route should not:

- duplicate fetch logic in multiple places
- fetch and manage one set of A2A tasks while a child fetches another

## Component Rules

- `A2ATaskTracker` becomes render-only
- `A2AAgentCard` gets a required action callback when used in orchestration
- trust graph render logic only consumes store data

## Cross-ADE Integration Rules

The orchestration page is a control-plane surface, not an island.

It must be able to trace back into:

- agents
- goals/proposals
- workflows
- sessions

Every orchestration entity shown to the operator should expose enough identifier context to navigate to the originating ADE surface.
