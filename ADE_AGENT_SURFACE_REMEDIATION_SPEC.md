# ADE Agent Surface Remediation Spec

Status: March 11, 2026

Purpose: define the authoritative remediation plan for the ADE agent surface so the agent list, agent detail view, safety controls, SDK, WebSocket events, and backend contracts operate as one coherent system.

This document is grounded in the live codebase. If implementation notes, older design prose, or local assumptions conflict with this document, this document wins.

Companion documents:

- `ADE_AGENT_SURFACE_IMPLEMENTATION_PLAN.md`
- `ADE_AGENT_SURFACE_VERIFICATION_PLAN.md`
- `ADE_AGENT_SURFACE_TASKS.md`
- `ADE_AGENT_SURFACE_AGENT_HANDOFF.md`

## Standard

This work is held to the following bar:

- No frontend view may infer authoritative agent state from partial data.
- No backend route may expose lifecycle state without exposing effective safety state.
- No page may present global data as agent-scoped data.
- No action may be shown in the UI unless the full success path is implemented.
- No SDK type may disagree with the live gateway contract.
- No WebSocket event taxonomy may require the UI to guess how to merge state.
- No panel may silently downgrade backend failure into "empty state" when that failure hides broken wiring.

## Scope

This spec covers:

- `dashboard/src/routes/agents/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/lib/stores/agents.svelte.ts`
- `packages/sdk/src/agents.ts`
- `packages/sdk/src/runtime-sessions.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`
- `crates/ghost-gateway/src/api/agents.rs`
- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- agent-surface tests in gateway, SDK, and dashboard layers

This spec does not redesign unrelated ADE areas such as Studio, Skills, Workflows, or Observability except where those systems define shared agent contracts.

## Current-State Diagnosis

The current agent surface is not one system. It is a stitched view assembled from partially compatible contracts.

The principal failure mode is not syntax or build breakage. The principal failure mode is semantic drift:

- the registry, SDK, and dashboard do not share one status model
- the safety system changes state without updating the agent-surface authority
- the detail page builds an agent view from unrelated list endpoints
- the detail page presents some global data as if it were per-agent
- some safety actions are displayed even though the UI cannot legally complete them

As a result, the ADE agent surface is not trustworthy enough to serve as the operator authority for agent runtime state.

## Confirmed Findings

### F1. There is no canonical agent status model.

The gateway registry stores lifecycle state as `Starting`, `Ready`, `Stopping`, `Stopped`.

The SDK only admits `Starting`, `Running`, `Stopping`, `Stopped`.

The dashboard treats `Running`, `Paused`, and `Quarantined` as user-facing states.

Implication:

- a healthy `Ready` agent is not modeled consistently
- pause and quarantine are not represented by the same contract that drives the list and detail pages
- every consumer is forced to improvise

### F2. Effective state is split between lifecycle and kill-switch state.

`/api/agents` currently serializes registry lifecycle, sandbox config, and sandbox metrics, but not the effective safety overlay.

Implication:

- the route that should be the ADE source of truth is incomplete
- the dashboard cannot know whether an agent is operational, paused, quarantined, or blocked by platform kill state

### F3. The list page does not subscribe to the full set of state-changing events.

The list page reloads on `AgentStateChange`, but pause and quarantine emit `KillSwitchActivation`.

Implication:

- the UI goes stale after safety actions
- users are forced to reload or wait for reconnect behavior

### F4. The detail page is assembled by client-side fan-out instead of a cohesive read model.

The detail view currently performs multiple independent requests and then tries to compose a truthful agent page in the browser.

Implication:

- there is no one authoritative definition of what an "agent detail view" is
- partial failures are hidden
- state races are possible because each panel has different freshness

### F5. "Recent Sessions" is not agent-scoped.

The detail page loads global sessions and slices the first page instead of asking the backend for sessions involving the active agent.

Implication:

- the detail page can display sessions unrelated to the agent being viewed
- the page is factually wrong even when it renders cleanly

### F6. Quarantine resume is exposed without a valid end-to-end flow.

The backend requires forensic review and second confirmation to resume from quarantine.

The current UI exposes a generic resume action without those inputs.

Implication:

- the UI offers an action path that is expected to fail
- operator trust is damaged because the surface suggests capability it does not actually possess

### F7. Error states are being converted into empty states.

Several detail-page dependencies are individually caught and replaced with `null` or `[]`.

Implication:

- contract breakage can be misread as "no data yet"
- the page hides integration faults instead of surfacing them

### F8. Shared agent store semantics are already drifting.

The shared agents store still treats `active` as a status even though the backend never emits it.

Implication:

- the drift is systemic, not isolated to one page
- command-palette and shared navigation integrations will continue to diverge unless the contract is fixed at the source

## Target System

The ADE agent surface must be built around one backend-owned operational model.

### Core Rule

The frontend does not compute agent truth.

The frontend renders backend-owned read models and sends backend-owned mutations.

### Canonical Agent Model

The system shall expose a canonical `AgentOperationalStatus` with three layers:

1. Lifecycle state
   - `starting`
   - `ready`
   - `stopping`
   - `stopped`

2. Safety state
   - `normal`
   - `paused`
   - `quarantined`
   - `kill_all_blocked`

3. Effective display state
   - `starting`
   - `ready`
   - `paused`
   - `quarantined`
   - `stopping`
   - `stopped`
   - `kill_all_blocked`

The precedence rule is:

- platform kill state overrides per-agent lifecycle for display purposes
- quarantine overrides pause
- pause overrides ready
- stopping and stopped remain terminal lifecycle displays

The gateway owns this derivation.

## Required Backend Read Models

### 1. Agent Summary

`GET /api/agents`

Returns list-card data only, but fully truthful:

- identity
- lifecycle state
- safety state
- effective display state
- spending cap summary
- convergence summary
- sandbox summary
- whether the agent is actionable from the UI

This route is the only source for the main agent grid.

### 2. Agent Detail

`GET /api/agents/:id`

Returns canonical base detail:

- identity
- lifecycle state
- safety state
- effective display state
- isolation
- capabilities
- sandbox config
- sandbox metrics
- action policy

`action_policy` must tell the UI which actions are currently allowed and what additional requirements apply.

### 3. Agent Overview Read Model

`GET /api/agents/:id/overview`

Returns the detail-page payload as one coherent backend view:

- `agent`
- `convergence`
- `cost`
- `recent_sessions`
- `recent_audit_entries`
- `crdt_summary`
- `integrity_summary`
- `panel_health`

This route may internally query multiple subsystems, but the browser must not.

### 4. Agent-Scoped Sessions

Sessions must be queryable by `agent_id`.

Two acceptable designs:

- extend `GET /api/sessions` with `agent_id`
- or expose `GET /api/agents/:id/sessions`

The implementation should prefer the smallest public surface that remains clear and reusable.

## Required Mutation Behavior

### Safety Mutations

Pause, quarantine, and resume remain safety-owned operations, but the agent surface must receive one consistent update stream after they succeed.

The event model shall be normalized so the UI can react without guessing:

- either emit one `AgentOperationalStatusChanged`
- or emit `AgentStateChange` for every effective-state mutation and reserve kill-switch events for operator/audit telemetry

The preferred design is a dedicated `AgentOperationalStatusChanged` event with:

- `agent_id`
- `lifecycle_state`
- `safety_state`
- `effective_state`
- `reason`
- `changed_at`

### Quarantine Resume

The UI shall not expose a generic resume path for quarantined agents.

Instead it must use a distinct gated flow:

- forensic review acknowledgment
- second confirmation
- capability or role check feedback
- explicit monitoring notice after resume

## Dashboard Design Rules

### Rule 1. No local filtering of global data to simulate agent scope.

If a panel is labeled as agent-specific, it must come from an agent-scoped backend read path.

### Rule 2. No action button without backend preconditions represented in the UI.

If resume-from-quarantine needs two confirmations and elevated permission, the UI must encode that, not discover it by failure.

### Rule 3. No "empty" state for upstream failure.

Each card must distinguish:

- loading
- empty
- unavailable
- error

### Rule 4. Shared status presentation must use one renderer.

The main list, detail view, shared store, command palette, and any other agent surfaces must consume the same status enum and label mapping.

## Architecture Decisions

### Decision A. Backend-owned overview route

Decision:

- create `GET /api/agents/:id/overview`

Reason:

- removes seven-way client fan-out
- ensures consistent freshness and error semantics
- gives future tests one stable agent-detail contract

### Decision B. Keep safety logic in safety routes, not agent routes

Decision:

- pause, resume, and quarantine remain in `api/safety.rs`

Reason:

- safety is a system concern, not merely a property-edit action
- keeps authorization, idempotency, and audit logic centralized

### Decision C. Move state derivation server-side

Decision:

- derive effective operational status in the gateway

Reason:

- only the gateway sees lifecycle and kill-switch state together
- avoids divergent precedence logic across clients

### Decision D. Treat agent detail as a product surface, not a debug collage

Decision:

- the detail page is a first-class read model with explicit panel health

Reason:

- operator-facing surfaces must be truthful under degradation
- diagnostic richness is acceptable, but only if scoped and owned

## Non-Negotiable Acceptance Criteria

This remediation is not complete until all of the following are true:

- `/api/agents` and the SDK expose one canonical operational status model
- the dashboard list and detail pages render the same status truth without local reinterpretation
- pause, quarantine, and resume transitions update the visible agent surface without page reload
- the detail page no longer displays unrelated sessions
- quarantined agents cannot be "resumed" through an invalid UI path
- panel failures are rendered explicitly as failures, not empty states
- generated types, SDK wrappers, and gateway routes are parity-checked
- gateway, SDK, and dashboard tests cover the primary happy paths and the primary degraded paths

## Out of Scope

The following are intentionally excluded from this remediation package:

- redesigning the broader observability IA
- changing quarantine policy semantics
- changing kill-switch persistence design
- redesigning convergence scoring itself
- redesigning audit schema beyond agent-surface read needs

## Recommended Execution Order

Build in this order:

1. canonical status contract
2. backend summary/detail/overview read models
3. normalized WebSocket event contract
4. SDK alignment
5. list-page rewrite onto canonical summary
6. detail-page rewrite onto overview route
7. quarantine-resume gated flow
8. end-to-end verification and parity gates

The implementation details for each step live in `ADE_AGENT_SURFACE_IMPLEMENTATION_PLAN.md`.
