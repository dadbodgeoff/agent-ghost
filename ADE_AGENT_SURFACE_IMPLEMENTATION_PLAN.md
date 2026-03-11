# ADE Agent Surface Implementation Plan

Status: March 11, 2026

Authoritative design: `ADE_AGENT_SURFACE_REMEDIATION_SPEC.md`
Verification companion: `ADE_AGENT_SURFACE_VERIFICATION_PLAN.md`
Execution tracker: `ADE_AGENT_SURFACE_TASKS.md`

## Document Intent

This document translates the remediation spec into an executable build plan.

Every phase below is ordered by dependency. Each work item identifies the primary files to change, the outcome required, and the implementation notes that should constrain decision-making.

## Delivery Strategy

The safest delivery model is incremental vertical slices:

1. backend truth first
2. SDK and generated types second
3. dashboard integration third
4. tests and parity gates at every layer

Do not begin by restyling the UI.

Do not begin by patching one page locally.

Begin by fixing the system authority.

## Phase 1: Establish the Canonical Agent Contract

### Goal

Create one backend-owned agent operational status model and make it the only status contract consumed by SDK and dashboard layers.

### Primary Files

- `crates/ghost-gateway/src/api/agents.rs`
- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/agents/registry.rs`
- `packages/sdk/src/agents.ts`
- `packages/sdk/src/websocket.ts`

### Required Changes

1. Introduce explicit status enums in the gateway response model:
   - `lifecycle_state`
   - `safety_state`
   - `effective_state`

2. Add one server-side derivation function that combines:
   - registry lifecycle state
   - per-agent kill-switch state
   - platform kill-all state

3. Preserve existing string compatibility only if necessary, but mark one field as canonical. Prefer introducing the new fields and removing ambiguous reliance on `status`.

4. Update OpenAPI so generated types reflect the real shape.

5. Update SDK wrappers to use the generated types or exact-compatible manual wrappers.

### Implementation Notes

- The gateway, not the dashboard, decides whether an agent is paused or quarantined.
- `Ready` is a lifecycle state, not a dashboard-only alias.
- If a compatibility field is retained temporarily, it must be clearly documented as derived and deprecated.

### Exit Criteria

- backend summary/detail payloads expose canonical status fields
- SDK types compile against the new contract
- status precedence is tested

## Phase 2: Build the Agent Read Models

### Goal

Replace browser-side composition with backend-owned agent read models.

### Primary Files

- `crates/ghost-gateway/src/api/agents.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/api/audit.rs`
- `crates/ghost-gateway/src/api/state.rs`
- `crates/ghost-gateway/src/api/integrity.rs`
- `crates/ghost-gateway/src/api/costs.rs`
- `packages/sdk/src/agents.ts`
- `packages/sdk/src/runtime-sessions.ts`

### Required Changes

1. Add `GET /api/agents/:id`.

2. Add `GET /api/agents/:id/overview`.

3. Define response types that include:
   - `agent`
   - `convergence`
   - `cost`
   - `recent_sessions`
   - `recent_audit_entries`
   - `crdt_summary`
   - `integrity_summary`
   - `panel_health`

4. Add agent-scoped session query support.

Preferred option:

- extend `GET /api/sessions` with `agent_id`

Fallback option:

- add `GET /api/agents/:id/sessions`

5. If session filtering depends on `sender`, document and test the exact semantics:
   - include sessions where the agent emitted at least one event
   - order by most recent event

### Implementation Notes

- The overview route is a read model, not a new domain source of truth.
- Keep internal composition behind the route boundary.
- `panel_health` is required so the UI can distinguish "empty" from "upstream unavailable."
- Do not silently swallow subsystem errors when constructing the overview response.

### Exit Criteria

- the detail page can be implemented from one route plus safety mutations
- agent-scoped sessions are truthful
- overview payload defines explicit panel error/availability semantics

## Phase 3: Normalize Agent-Surface WebSocket Events

### Goal

Ensure all state-changing operations produce one reliable real-time contract for the agent surface.

### Primary Files

- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-gateway/src/api/agents.rs`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/lib/stores/agents.svelte.ts`

### Required Changes

1. Introduce one authoritative event for agent operational-state changes.

Recommended shape:

- `type: "AgentOperationalStatusChanged"`
- `agent_id`
- `lifecycle_state`
- `safety_state`
- `effective_state`
- `reason`
- `changed_at`

2. Emit that event for:
   - agent creation
   - agent update that changes effective state
   - pause
   - quarantine
   - resume
   - delete

3. Keep `KillSwitchActivation` if needed for audit/telemetry consumers, but agent list/detail logic must not rely on it as a substitute for state sync.

4. Update the shared agent store to consume the new event instead of guessing from partial payloads.

### Implementation Notes

- Real-time contracts should reduce reloads, not force more of them.
- If the event stream cannot carry full new state, it is insufficient for the agent surface.

### Exit Criteria

- list and detail surfaces stay correct after pause/quarantine/resume without manual reload
- shared store logic no longer depends on undefined statuses such as `active`

## Phase 4: Rewrite the Agent List Surface

### Goal

Move the main agents page to the canonical summary contract.

### Primary Files

- `dashboard/src/routes/agents/+page.svelte`
- `dashboard/src/lib/stores/agents.svelte.ts`
- `dashboard/src/components/StatusBadge.svelte`
- `packages/sdk/src/agents.ts`

### Required Changes

1. Render card state from canonical backend summary fields.

2. Use one shared status renderer and label mapping for:
   - effective state label
   - effective state color
   - actionability hints

3. Ensure the page reacts to:
   - initial REST load
   - operational status WebSocket updates
   - resync

4. Remove stale semantics such as:
   - `Running` as the only healthy state
   - `active` in shared store logic

### Implementation Notes

- The list surface is an operator surface, not just a gallery.
- If action affordances are added later, they must use the same action policy as the detail surface.

### Exit Criteria

- list page is truthful for ready, paused, quarantined, stopping, stopped, and kill-all-blocked states

## Phase 5: Rewrite the Agent Detail Surface

### Goal

Turn the detail view into a cohesive agent product surface rather than a client-side debug composition.

### Primary Files

- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/components/ConfirmDialog.svelte`
- `packages/sdk/src/agents.ts`
- `packages/sdk/src/safety.ts`

### Required Changes

1. Replace multi-endpoint fan-out with:
   - `GET /api/agents/:id`
   - `GET /api/agents/:id/overview`

2. Render explicit panel states:
   - loading
   - empty
   - unavailable
   - error

3. Render recent sessions only from agent-scoped data.

4. Remove local status heuristics that reinterpret backend values.

5. Reflect action policy from the backend:
   - whether pause is allowed
   - whether quarantine is allowed
   - whether resume is allowed
   - whether resume requires gated workflow

### Implementation Notes

- The detail surface can remain technically rich, but each panel must be owned and truthful.
- "No data" is not acceptable when the overview route explicitly reports upstream failure.

### Exit Criteria

- the detail page uses backend-owned data models only
- no unrelated sessions appear
- panel failures are explicit

## Phase 6: Implement the Quarantine Resume Flow

### Goal

Provide a valid UI path for quarantined-agent resume instead of a knowingly incomplete button.

### Primary Files

- `dashboard/src/routes/agents/[id]/+page.svelte`
- `packages/sdk/src/safety.ts`
- `crates/ghost-gateway/src/api/safety.rs`

### Required Changes

1. Split resume behavior into:
   - resume from pause
   - resume from quarantine

2. Build a gated quarantine-resume interaction containing:
   - forensic review acknowledgment
   - second confirmation
   - clear post-resume monitoring notice

3. Only show the gated flow when:
   - the backend action policy says it is allowed

4. Show explicit failure reasons from the backend when authorization blocks the action.

### Implementation Notes

- A single generic "Resume" button is not acceptable once the system has multiple resume semantics.
- If role/capability awareness is not available in the dashboard, the backend action policy should communicate the expected failure mode clearly enough for the UI to message it.

### Exit Criteria

- paused agents can be resumed simply
- quarantined agents can only be resumed through the gated path

## Phase 7: Contract Gates and Cleanup

### Goal

Prevent recurrence of the same contract drift.

### Primary Files

- `scripts/check_openapi_parity.py`
- `scripts/check_generated_types_freshness.py`
- gateway tests
- SDK tests
- Playwright tests

### Required Changes

1. Ensure generated types are refreshed and committed.

2. Add tests that fail if:
   - backend status enums drift from SDK types
   - overview route loses required fields
   - sessions filtering regresses
   - safety events stop updating the agent surface

3. Remove dead or misleading status handling from dashboard stores and components.

### Exit Criteria

- contract drift is caught before merge
- the implementation is simpler than the current stitched behavior

## Recommended Branch Sequence

If executed in multiple commits:

1. status-contract backend + SDK
2. overview/session backend routes
3. WebSocket normalization + shared store
4. agents list page
5. agent detail page
6. quarantine resume flow
7. tests and parity gates

## Final Deliverable

The final system should let an operator:

- open `/agents`
- see truthful state without refresh
- open `/agents/:id`
- trust every panel to be agent-scoped and explicit about degradation
- pause, quarantine, and resume agents through valid paths only

That is the definition of "cohesive unit" for this remediation.
