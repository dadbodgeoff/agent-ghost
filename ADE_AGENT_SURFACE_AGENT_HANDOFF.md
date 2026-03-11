# ADE Agent Surface Agent Handoff

Status: March 11, 2026

Use this document as the execution brief for the implementation agent.

Authoritative design: `ADE_AGENT_SURFACE_REMEDIATION_SPEC.md`
Implementation plan: `ADE_AGENT_SURFACE_IMPLEMENTATION_PLAN.md`
Verification plan: `ADE_AGENT_SURFACE_VERIFICATION_PLAN.md`
Execution tracker: `ADE_AGENT_SURFACE_TASKS.md`

## Mission

Remediate the ADE agent surface so the agent list, agent detail page, safety controls, SDK contracts, and gateway contracts behave as one coherent product surface.

The current system is not allowed to remain a client-side composition of partially compatible contracts.

Your job is to replace that with one backend-owned operational model and one truthful dashboard integration.

## Non-Negotiable Outcomes

You must deliver all of the following:

1. one canonical agent operational status contract
2. backend summary, detail, and overview read models for agent surfaces
3. real-time event wiring that updates agent surfaces after pause, quarantine, resume, create, and delete
4. agent-scoped recent sessions on the detail page
5. a valid quarantine-resume UI flow with required acknowledgments
6. explicit degraded/error panel states instead of silent empty states
7. gateway, SDK, and dashboard tests that pin the above behavior down

## Hard Constraints

- Do not leave status interpretation in the frontend.
- Do not keep the current multi-endpoint detail-page fan-out.
- Do not present global sessions as agent-scoped data.
- Do not expose action buttons that the UI cannot successfully complete.
- Do not add a compatibility shortcut that preserves drift unless it is temporary, explicit, and tested.

## Build Order

Execute in this order:

1. Canonicalize the backend agent status contract.
2. Add `GET /api/agents/:id` and `GET /api/agents/:id/overview`.
3. Add agent-scoped session query support.
4. Normalize WebSocket event semantics for agent operational status.
5. Align SDK types and wrappers with generated contract output.
6. Rewrite the agents list page to use canonical status.
7. Rewrite the agent detail page to use the overview read model.
8. Implement the gated quarantine-resume flow.
9. Add and run verification coverage across gateway, SDK, and dashboard.

## Primary Files To Touch

Backend:

- `crates/ghost-gateway/src/api/agents.rs`
- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/api/websocket.rs`

SDK:

- `packages/sdk/src/agents.ts`
- `packages/sdk/src/runtime-sessions.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`

Dashboard:

- `dashboard/src/routes/agents/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/lib/stores/agents.svelte.ts`
- any shared status-rendering component needed for consistency

Tests:

- gateway tests for agent APIs and events
- SDK client tests
- `dashboard/tests/agents.spec.ts`

## Exact Behavioral Requirements

### Agent Status

The gateway must expose:

- `lifecycle_state`
- `safety_state`
- `effective_state`

The frontend must render those values, not reinterpret legacy strings.

### List Page

The list page must:

- render truthful effective state
- update without full-page reload after safety actions
- use one shared label/color mapping

### Detail Page

The detail page must:

- load from backend-owned detail and overview models
- show only agent-scoped recent sessions
- show explicit panel health states
- show only valid actions

### Quarantine Resume

The UI must not use the plain pause-resume path for quarantined agents.

It must:

- collect forensic-review acknowledgment
- collect second confirmation
- send the required backend payload
- message post-resume monitoring clearly

## Verification Requirements

Before declaring completion, run the checks required by `ADE_AGENT_SURFACE_VERIFICATION_PLAN.md`.

At minimum, the change is not complete until:

- dashboard type-check passes
- relevant gateway tests pass
- SDK tests for changed wrappers pass
- dashboard e2e tests cover the remediated flows

## Definition Of Success

An ADE operator can:

1. open `/agents`
2. trust the displayed state
3. open `/agents/:id`
4. trust that every panel refers to that agent
5. pause, quarantine, and resume agents through valid flows
6. see degraded backend dependencies as degraded, not silently empty

If any of those remain false, the mission is incomplete.
