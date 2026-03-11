# ADE Agent Surface Tasks

Status: March 11, 2026

Authoritative design: `ADE_AGENT_SURFACE_REMEDIATION_SPEC.md`
Implementation companion: `ADE_AGENT_SURFACE_IMPLEMENTATION_PLAN.md`
Verification companion: `ADE_AGENT_SURFACE_VERIFICATION_PLAN.md`
Final handoff brief: `ADE_AGENT_SURFACE_AGENT_HANDOFF.md`

## Execution Rules

- Do not start frontend cleanup before backend contract work lands.
- Do not merge UI work that still depends on client-side interpretation of status truth.
- Do not close a task without the tests named in the verification plan.

## Phase 1: Canonical Contract

- [ ] Define canonical lifecycle, safety, and effective-state fields in gateway agent response types.
- [ ] Implement server-side derivation of effective agent state from registry plus kill-switch state.
- [ ] Update `/api/agents` to emit canonical status fields.
- [ ] Add `GET /api/agents/:id`.
- [ ] Update OpenAPI definitions for agent routes.
- [ ] Regenerate SDK types.
- [ ] Update SDK agent types and wrappers to align with the new contract.
- [ ] Add gateway tests for status derivation precedence.

## Phase 2: Cohesive Read Models

- [ ] Add `GET /api/agents/:id/overview`.
- [ ] Define `panel_health` contract for overview payloads.
- [ ] Add agent-scoped session query support.
- [ ] Ensure recent sessions are ordered and agent-scoped.
- [ ] Add gateway tests for overview payload shape and session scoping.
- [ ] Add SDK wrappers for detail and overview routes.

## Phase 3: Real-Time Synchronization

- [ ] Add authoritative operational-status WebSocket event.
- [ ] Emit the event for create, pause, quarantine, resume, delete, and any relevant state mutation.
- [ ] Update SDK WebSocket event types.
- [ ] Update shared dashboard agent store to consume the new event.
- [ ] Remove dead `active` status semantics from shared store logic.
- [ ] Add gateway and SDK tests for event shape and emission.

## Phase 4: Agent List UI

- [ ] Rewrite list page to render canonical effective state.
- [ ] Unify state label/color rendering through one shared path.
- [ ] Verify list page responds to real-time operational-status events.
- [ ] Add Playwright coverage for list-page state transitions.

## Phase 5: Agent Detail UI

- [ ] Rewrite detail page to use detail and overview read models instead of client-side fan-out.
- [ ] Replace silent per-panel `.catch` degradation with explicit unavailable/error states.
- [ ] Render only agent-scoped recent sessions.
- [ ] Render action policy returned by the backend.
- [ ] Add Playwright coverage for detail-page data truthfulness and degradation handling.

## Phase 6: Quarantine Resume Flow

- [ ] Separate pause-resume and quarantine-resume interactions in the UI.
- [ ] Implement forensic review acknowledgment flow.
- [ ] Implement second confirmation flow.
- [ ] Surface authorization/policy constraints clearly.
- [ ] Add tests for quarantine-resume success and blocked paths.

## Phase 7: Final Hardening

- [ ] Remove obsolete local status heuristics from dashboard routes and stores.
- [ ] Refresh generated types and confirm no drift.
- [ ] Run gateway tests for touched routes.
- [ ] Run SDK tests for touched wrappers.
- [ ] Run dashboard check.
- [ ] Run dashboard agent-surface e2e coverage.
- [ ] Confirm manual integration checklist from the verification plan.

## Done Definition

This work is done only when:

- the list and detail pages are backend-truthful
- the safety flows are live and valid end to end
- the detail page no longer presents unrelated global data as agent data
- the tests and parity gates prevent regression
