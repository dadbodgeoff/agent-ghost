# Task: Rebuild the ADE Sessions Subsystem End to End

## Mission

Implement the runtime Sessions subsystem so it operates as one coherent ADE surface from gateway to SDK to dashboard.

Do not patch symptoms in isolated routes. Fix the subsystem as a unit.

Primary references:

- `/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/sessions-remediation/design.md`
- `/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/sessions-remediation/contracts.md`
- `/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/sessions-remediation/validation.md`

## Operating Standard

Work like this surface is safety-adjacent operator infrastructure.

- do not accept silent truncation
- do not accept mixed pagination contracts
- do not accept array-index semantics where sequence semantics are required
- do not accept optimistic UI that can diverge from persisted truth
- do not accept cross-surface session claims that are not backed by canonical data
- do not preserve SDK or dashboard drift from generated contracts

## Scope

In scope:

- runtime session list, detail, replay, bookmarks, and branch flows
- gateway routes and OpenAPI for runtime sessions
- SDK runtime session wrappers
- shared dashboard session store
- Agents and Observability session integrations
- test coverage for list, replay, mutation, and refresh behavior

Out of scope:

- Studio session redesign
- unrelated dashboard modernization
- storage-engine replacement

## Required Deliverables

### D1. Canonical runtime-session API

Deliver:

- one cursor-based list contract
- `GET /api/sessions/:id`
- sequence-based event, bookmark, and branch semantics
- `agent_ids: string[]` in public payloads

Required fixes:

- bookmark delete must enforce `(session_id, bookmark_id)`
- branch must reject missing or zero-copy checkpoints
- audit lineage must record the real owning session

### D2. OpenAPI and SDK parity

Deliver:

- runtime session routes documented in OpenAPI with canonical request and response shapes
- regenerated session-related generated types
- SDK wrappers aligned exactly to generated contracts

### D3. Shared dashboard data path

Deliver:

- `/sessions` uses the shared store
- store owns cursor pagination, refresh, and error state
- store refreshes on websocket resync
- detail and replay use canonical summary and event contracts

### D4. Replay correctness

Deliver:

- bookmarks use `sequence_number`
- branch requests use `from_sequence_number`
- bookmark create and delete reflect server-confirmed state only
- replay surfaces mutation failures instead of swallowing them

### D5. ADE cohesion

Deliver:

- Agents page filters sessions by `agent_ids`
- Observability session picker uses the same normalized session model as `/sessions`
- route-to-route navigation preserves the same runtime session identity everywhere

### D6. Validation

Deliver:

- gateway tests for cursor ordering, bookmark ownership, and branch validation
- SDK contract tests for regenerated session types
- dashboard tests for pagination, resync refresh, replay mutation failure, and agent filtering

## Exact Files To Inspect and Change

- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/ghost-gateway/tests/operation_journal_tests.rs`
- `crates/ghost-gateway/tests/test_critical_path.rs`
- `packages/sdk/src/runtime-sessions.ts`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/__tests__/client.test.ts`
- `dashboard/src/lib/stores/sessions.svelte.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/routes/sessions/+page.svelte`
- `dashboard/src/routes/sessions/[id]/+page.svelte`
- `dashboard/src/routes/sessions/[id]/replay/+page.svelte`
- `dashboard/src/routes/agents/[id]/+page.svelte`
- `dashboard/src/routes/observability/+page.svelte`
- `dashboard/tests`

## Execution Order

1. Lock the backend contract from `contracts.md`.
2. Update gateway handlers and tests.
3. Update OpenAPI and regenerate types.
4. Align the SDK wrappers and SDK tests.
5. Rewire the dashboard onto the shared store.
6. Repair Replay semantics and error handling.
7. Repair Agents and Observability integration.
8. Run the full validation set from `validation.md`.

## Non-Negotiables

- do not keep both page-based and cursor-based list payloads in active dashboard use
- do not keep comma-separated `agents` in public runtime session payloads
- do not keep bookmark or branch mutation semantics keyed to UI array index
- do not leave route-local list fetching in `/sessions`
- do not swallow replay mutation failures
- do not claim completion without dashboard tests for the repaired flows

## Definition of Done

The work is done only when:

- a runtime session can be listed, paginated, opened, replayed, bookmarked, branched, and revisited with consistent truth
- Agents and Observability show the same session identity and ownership story as `/sessions`
- the SDK, OpenAPI, gateway, and dashboard all agree on the contract without compensating adapters
- all required tests and manual validation steps pass
