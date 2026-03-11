# ADE Observability Agent Handoff

Date: March 11, 2026

Status: Build brief

Read first:
- [OBSERVABILITY_ADE_CURRENT_STATE_AUDIT.md](./OBSERVABILITY_ADE_CURRENT_STATE_AUDIT.md)
- [OBSERVABILITY_ADE_TARGET_ARCHITECTURE.md](./OBSERVABILITY_ADE_TARGET_ARCHITECTURE.md)
- [OBSERVABILITY_ADE_IMPLEMENTATION_PLAN.md](./OBSERVABILITY_ADE_IMPLEMENTATION_PLAN.md)
- [OBSERVABILITY_ADE_VALIDATION_GATES.md](./OBSERVABILITY_ADE_VALIDATION_GATES.md)

This document is the single brief to hand an implementation agent.

## Mission

Build ADE observability end to end so it is:
- truthful
- fully wired
- navigationally integrated
- contract-authoritative
- regression-tested

Do not optimize for speed by preserving known falsehoods.

## Required End State

When this work is complete:

1. ADE observability has a canonical public route:
   - `GET /api/observability/ade`
2. OpenAPI, generated SDK, backend, and dashboard all use the same response shape.
3. The dashboard exposes observability as a coherent area with:
   - `Traces`
   - `ADE Health`
4. The ADE Health page renders only real telemetry or explicit unavailability.
5. Degraded, recovering, stale, and unavailable states remain visible through the full stack.
6. Automated tests fail if the contract drifts or the UI regresses into false-green behavior.

## Architectural Constraints

- Keep `/api/health` and `/api/ready` as probe-oriented surfaces.
- Put rich ADE operator detail in `/api/observability/ade`.
- OpenAPI is the root public authority.
- Generated SDK types are authoritative over handwritten shadow types.
- No hardcoded subsystem health labels may remain in the ADE page.
- Do not leave `/observability/ade` unreachable from the main observability path.

## Execution Order

### Step 1. Create the authoritative backend contract

Implement:
- route definition
- response schema
- OpenAPI annotation

Target result:
- a typed `AdeObservabilitySnapshot` response exists and is published

### Step 2. Instrument and aggregate subsystem truth

Implement aggregation for:
- gateway uptime and FSM state
- monitor status and uptime
- active and registered agent counts
- websocket active connection count
- database size and WAL state
- backup scheduler status
- config watcher status
- existing autonomy, convergence protection, distributed kill, and speculative context summaries

Target result:
- every ADE page field has a real source or explicit `null`

### Step 3. Regenerate and normalize the SDK

Implement:
- OpenAPI regeneration
- typed SDK wrapper for the new route
- cleanup of any conflicting handwritten types

Target result:
- no contract-name drift remains

### Step 4. Rebuild observability route architecture

Implement:
- observability sub-navigation or layout
- `Traces` route
- `ADE Health` route
- default route behavior that is coherent

Target result:
- both observability surfaces are reachable in a normal operator flow

### Step 5. Rebuild the ADE Health page on the canonical snapshot

Implement:
- truthful component rows
- explicit degraded and stale handling
- automatic refresh semantics
- removal of hardcoded success labels

Target result:
- the page behaves like an operator tool, not a static mockup

### Step 6. Add regression gates

Implement:
- backend tests
- SDK tests
- dashboard route and rendering tests
- parity checks where needed

Target result:
- drift cannot silently re-enter

## Files Likely To Change

Backend:
- `crates/ghost-gateway/src/api/observability.rs` (new)
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/config_watcher.rs`
- `crates/ghost-gateway/src/backup_scheduler.rs`
- `crates/ghost-gateway/src/db_pool.rs`
- `crates/convergence-monitor/src/transport/http_api.rs`

SDK:
- `packages/sdk/src/observability.ts` (new)
- `packages/sdk/src/index.ts`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/__tests__/client.test.ts`

Dashboard:
- `dashboard/src/routes/observability/+layout.svelte` (new)
- `dashboard/src/routes/observability/+page.svelte` or redirect replacement
- `dashboard/src/routes/observability/traces/+page.svelte` (new or moved)
- `dashboard/src/routes/observability/ade/+page.svelte`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/tests/observability.spec.ts` (new)

## Definition Of Done

The work is done only when all of the following are true:

1. `/api/observability/ade` exists and is typed in OpenAPI.
2. The SDK uses generated contract types for the route.
3. The ADE Health page contains no hardcoded health assertions such as fixed "Reachable", "Scheduled", or "Watching" states without backing fields.
4. A degraded monitor renders degraded in the UI.
5. A stale snapshot renders stale in the UI.
6. Observability navigation exposes both traces and ADE health.
7. Automated tests cover healthy, degraded, stale, and failure states.
8. The implementation passes the validation gates in [OBSERVABILITY_ADE_VALIDATION_GATES.md](./OBSERVABILITY_ADE_VALIDATION_GATES.md).

## Failure Modes To Avoid

- extending the current handwritten frontend `HealthData` type and calling that a fix
- stuffing ADE operator detail into `/api/health`
- leaving the OpenAPI response body unspecified
- preserving current false-green heuristics
- using local UI inference where the backend should provide canonical status
- shipping without navigation integration

## Deliverable Expected From The Agent

Produce:
- code changes
- tests
- regenerated contracts
- a short closeout note listing:
  - what changed
  - what was validated
  - any intentional exceptions that remain

If any required telemetry source cannot be produced cleanly, the agent must stop hiding that gap and expose it explicitly as unavailable in both contract and UI.
