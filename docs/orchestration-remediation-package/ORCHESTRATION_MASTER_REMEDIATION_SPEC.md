# Orchestration Master Remediation Spec

Status: March 11, 2026
Purpose: define the authoritative remediation plan to bring ADE orchestration to a strict production engineering bar across backend contracts, persistence, SDK, dashboard state, and realtime behavior.

This document is based on the live code. If this spec conflicts with older design notes, older orchestration assumptions, or placeholder dashboard behavior, this spec wins.

## Standard

This work is held to the following non-negotiable bar:

- No orchestration panel without one explicit data authority.
- No dashboard state duplicated between route and child component for critical control-plane data.
- No typed SDK wrapper that forks from backend contract shape without an explicit exception record.
- No query error swallowed if it can invalidate operator trust.
- No graph, consensus, or sybil visualization that is not backed by real persisted semantics.
- No "live" orchestration label unless websocket, replay, and reconnect semantics keep the surface materially accurate.
- No placeholder metric may survive behind production styling.
- No hidden dependency on bootstrapped local state when the UI promises network discovery.

## Scope

This spec covers:

- orchestration backend routes in `crates/ghost-gateway`
- orchestration persistence dependencies in `crates/cortex/cortex-storage`
- orchestration websocket event consumption
- orchestration SDK contract surfaces in `packages/sdk`
- orchestration dashboard route, components, and shared stores in `dashboard/src`
- orchestration test gates across gateway, SDK, and dashboard

This spec covers four orchestration domains:

1. Trust graph
2. Consensus state
3. Delegation and sybil posture
4. A2A discovery and task execution tracking

## Primary Sources

- `crates/ghost-gateway/src/api/mesh_viz.rs`
- `crates/ghost-gateway/src/api/a2a.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/cortex/cortex-storage/src/migrations/v018_delegation_state.rs`
- `crates/cortex/cortex-storage/src/migrations/v028_a2a_tasks.rs`
- `crates/cortex/cortex-storage/src/migrations/v017_convergence_tables.rs`
- `crates/cortex/cortex-storage/src/migrations/v046_goal_proposal_v2.rs`
- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- `packages/sdk/src/mesh.ts`
- `packages/sdk/src/a2a.ts`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/routes/orchestration/+page.svelte`
- `dashboard/src/components/A2ATaskTracker.svelte`
- `dashboard/src/components/A2AAgentCard.svelte`
- `dashboard/src/lib/stores/websocket.svelte.ts`

## Confirmed Findings

### F1. Trust graph edges are not derived from the real delegation schema.

The current edge query references state and columns that do not match the persisted delegation table, and query preparation failure is suppressed.

Implication:

- the trust graph can present a graph shell without actual trust relationships
- operators cannot distinguish "no edges exist" from "the query is wrong"
- the system undermines confidence in the orchestration surface

### F2. Consensus numbers are not backed by a real consensus model.

The current route counts `approved` and `rejected` by re-reading the same proposal row rather than reading a vote or transition system.

Implication:

- progress bars are semantically false
- the orchestration tab diverges from the goals/proposal lifecycle model used elsewhere
- any operational decision made from this panel is suspect

### F3. Realtime orchestration coherence is broken.

The dashboard route loads non-A2A orchestration data once and does not maintain coherence under live mutations.

Implication:

- orchestration becomes stale while agents, proposals, and delegations continue changing
- reconnect/resync only partially restores the page
- the tab cannot serve as the operator truth surface

### F4. A2A execution tracking stops at dispatch.

The system records local submission but does not reconcile remote task lifecycle progression into a durable local execution model.

Implication:

- "in-flight task" language overstates reality
- SSE and task status models imply a lifecycle the gateway does not maintain
- operators cannot use the panel to manage or inspect actual remote execution state

### F5. UI state ownership is fragmented.

The route owns A2A count state while a child component owns the task table state with a separate fetch path.

Implication:

- the same panel can display contradictory state at the same time
- silent fetch failure can masquerade as "no tasks"
- realtime updates cannot stay consistent

### F6. Sybil posture is under-modeled.

The current metrics are mostly counts with at least one hardcoded value and no real topology analysis.

Implication:

- the panel looks decision-grade while remaining mostly decorative
- operators are given false confidence about delegation safety

## Target State

The completed orchestration subsystem must satisfy all of the following:

- every orchestration panel is backed by one explicit backend contract and one explicit frontend state owner
- every backend contract has an explicit derivation path from persisted data or live runtime state
- trust graph edges represent actual trust/delegation relationships with explainable weighting
- consensus view reflects the actual proposal lifecycle state model used by ADE
- sybil posture reflects real delegation topology characteristics, not placeholders
- A2A discovery, dispatch, and execution state form one coherent workflow
- websocket updates and resync produce consistent state across all orchestration panels
- dashboard, SDK, and backend contracts are test-gated against drift

## Required Invariants

### Trust

- Every displayed edge must correspond to one explicit persisted or computed trust relationship.
- Every displayed weight must have a deterministic derivation rule.
- Empty edge sets must mean "no relationships," not "query failed."

### Consensus

- Every displayed status must map to the canonical proposal lifecycle state machine.
- Every displayed count must be a real count over real records, not a convenience approximation.
- Threshold semantics must be explicitly defined and sourced.

### Sybil

- Delegation topology metrics must be computed from graph structure.
- Max chain depth must be computed, not hardcoded.
- Risk signals must separate structural risk from simple volume counts.

### A2A

- Discovery must distinguish configured peers, discovered peers, verified peers, and reachable peers.
- Dispatch must not be presented as completion.
- Task state must be reconciled into a durable lifecycle or the contract must explicitly stay fire-and-forget.

### Frontend

- One route-level orchestration store is the only source of truth for orchestration state.
- Child components are render-only or action-only; they do not refetch critical state behind the route.
- Realtime updates must either patch state deterministically or trigger bounded refetch of the affected slice.

## Explicit Non-Goals

- redesigning unrelated ADE surfaces
- inventing a new multi-agent consensus algorithm beyond what current ADE semantics require
- building a brand new mesh transport
- adding speculative AI features to orchestration before correctness is solved

## Definition Of Complete

Orchestration remediation is complete only when:

- backend routes are semantically aligned with real storage/runtime models
- SDK contract types reflect backend truth
- dashboard state is unified and realtime-safe
- acceptance tests prove the orchestration tab remains accurate across create/update/reconnect/resync paths
- the orchestration page can be trusted by an operator as the live control-plane surface
