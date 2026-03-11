# Sessions Remediation Execution Plan

## Objective

Deliver a fully wired Sessions subsystem without contract drift, misleading UI state, or cross-surface inconsistency.

## Phase Order

### Phase 0: Freeze the contract surface

Outputs:

- confirm current route inventory
- confirm generated type regeneration path
- capture existing session tests and gaps

Exit:

- implementation team agrees on `contracts.md`

### Phase 1: Backend contract hardening

Work:

- unify list response to one cursor contract
- add `GET /api/sessions/:id`
- convert bookmark and branch semantics from index-based to sequence-based
- enforce bookmark ownership on delete
- reject zero-copy branch attempts
- normalize `agent_ids` at the API boundary
- repair audit lineage for bookmark and branch mutations

Exit:

- gateway tests cover negative paths and invariants

### Phase 2: OpenAPI and SDK alignment

Work:

- update OpenAPI route docs
- regenerate generated types
- remove hand-written SDK drift for runtime sessions
- add typed wrappers only where they are exact transport helpers

Exit:

- SDK compiles against generated types without session-specific forks

### Phase 3: Frontend unification

Work:

- route `/sessions` onto the shared store
- add cursor pagination or infinite load
- ensure websocket resync refresh
- detail and replay consume canonical summary/detail contracts
- remove optimistic bookmark state lies
- switch replay checkpoint logic to `sequence_number`

Exit:

- `/sessions`, detail, and replay all use one canonical data path

### Phase 4: Cross-surface repair

Work:

- fix Agents page session filtering
- align Observability session picker with shared normalization
- confirm command palette and deep links still resolve

Exit:

- navigating from Agents or Observability to Sessions preserves truth

### Phase 5: Validation and ship gate

Work:

- backend tests
- SDK tests
- dashboard component and route tests
- replay mutation tests
- regression tests for stale UI and silent truncation

Exit:

- `validation.md` is satisfied in full

## Dependency Notes

- Phase 1 must land before any serious frontend work, otherwise the UI will be built on unstable semantics.
- Phase 2 must land before dashboard route rewiring, otherwise the frontend will keep compensating for backend drift.
- Phase 4 is not optional cleanup. It closes the ADE cohesion problem that caused the original audit findings.

## Cut Lines

Acceptable first cut:

- shared store list wiring
- cursor pagination
- correct bookmark ownership
- correct branch validation
- agent-page session filtering
- replay using server-confirmed bookmark state

Not acceptable as a stopping point:

- keeping mixed page and cursor list responses
- keeping comma-separated `agents` strings in the public API
- keeping array-index bookmark and branch semantics
- leaving Agents or Observability on divergent session normalization
