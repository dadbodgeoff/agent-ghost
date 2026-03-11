# ADE Observability Implementation Plan

Date: March 11, 2026

Status: Execution plan

## Implementation Principle

Build the authority chain in this order:

1. backend contract
2. backend aggregation and instrumentation
3. OpenAPI and generated SDK
4. dashboard route architecture
5. dashboard rendering and live refresh
6. validation and release gates

Do not start with dashboard polish.

## Phase 1. Establish Contract Authority

### Deliverables

- new typed public schema for `GET /api/observability/ade`
- OpenAPI declaration with explicit response body
- generated SDK types for the route
- SDK wrapper for the new endpoint

### Likely files

- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/ghost-gateway/src/api/observability.rs` (new)
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/observability.ts` (new)
- `packages/sdk/src/index.ts`

### Acceptance criteria

- the route has a named schema, not raw JSON
- the generated SDK contains the response body type
- there is no manual contract shadowing for this endpoint

## Phase 2. Instrument And Aggregate Runtime Truth

### Deliverables

- gateway-owned aggregation layer for ADE observability
- process uptime
- active and registered agent counts
- websocket active connection count
- database size and WAL status
- backup scheduler health state
- config watcher health state
- monitor status and uptime integrated into the aggregated snapshot

### Likely files

- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/config_watcher.rs`
- `crates/ghost-gateway/src/backup_scheduler.rs`
- `crates/ghost-gateway/src/db_pool.rs`
- `crates/ghost-gateway/src/api/observability.rs` (new)
- `crates/convergence-monitor/src/transport/http_api.rs`

### Implementation notes

- add a total-connection accessor to `WsConnectionTracker`
- record gateway process start time in app state
- expose database path from the configured pool or runtime state
- add health bookkeeping to backup scheduler and config watcher instead of scraping logs
- keep monitor polling bounded and cached

### Acceptance criteria

- every field in `AdeObservabilitySnapshot` has a real source
- any field without a real source is explicitly nullable and labeled unavailable
- no component status is derived from unrelated liveness state

## Phase 3. Normalize Existing Health Surfaces

### Deliverables

- clarify boundary between `/api/health`, `/api/ready`, and `/api/observability/ade`
- remove naming drift such as `distributed_gate` vs `distributed_kill`
- ensure gateway and dashboard use the same status naming

### Likely files

- `crates/ghost-gateway/src/api/health.rs`
- `packages/sdk/src/health.ts`
- `packages/sdk/src/generated-types.ts`
- any dashboard consumers of health status

### Acceptance criteria

- probe endpoints remain lean and probe-oriented
- operator endpoint is the rich ADE detail surface
- field names are consistent across backend, SDK, and dashboard

## Phase 4. Rebuild Observability Information Architecture

### Deliverables

- observability layout with explicit sub-navigation
- traces route under observability
- ADE health route under observability
- default route behavior that does not strand ADE health

### Likely files

- `dashboard/src/routes/observability/+layout.svelte` (new)
- `dashboard/src/routes/observability/+page.svelte` or redirect replacement
- `dashboard/src/routes/observability/traces/+page.svelte` (new or moved)
- `dashboard/src/routes/observability/ade/+page.svelte`
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/lib/stores/tabs.svelte.ts` if explicit tab behavior is desired

### Acceptance criteria

- operators can reach both traces and ADE health through the normal app path
- breadcrumbs and active nav state remain coherent
- no dead-end route remains

## Phase 5. Rebuild ADE Health UI On Top Of The Canonical Snapshot

### Deliverables

- the page consumes `AdeObservabilitySnapshot`
- no hardcoded health text remains
- component rows derive from real fields
- stale, degraded, recovering, and unavailable states are explicit

### Likely files

- `dashboard/src/routes/observability/ade/+page.svelte`
- optional shared component files under `dashboard/src/components/`

### Required UI behavior

- a refresh control remains available
- live data refresh is automatic via poll, websocket-triggered refresh, or both
- stale data is labeled stale
- last-error messaging is visible for failing subsystems where available

### Acceptance criteria

- if the monitor disconnects, the page shows monitor degraded without painting the gateway healthy-green by implication
- if the websocket subsystem has zero or unavailable connection metrics, the UI reports zero or unavailable, not healthy by default
- if the backend request fails, the page does not continue to show a hardcoded healthy API badge

## Phase 6. Add Regression Gates

### Deliverables

- backend contract tests for `/api/observability/ade`
- SDK tests for response shape
- dashboard tests for healthy and degraded rendering
- parity checks that fail when OpenAPI and SDK drift

### Likely files

- `crates/ghost-gateway/tests/`
- `packages/sdk/src/__tests__/client.test.ts`
- `dashboard/tests/observability.spec.ts` (new)
- optional parity scripts under `scripts/`

### Acceptance criteria

- drift in field names or requiredness fails CI
- the dashboard has coverage for healthy, degraded, stale, and unavailable states

## Ordered Execution Sequence

1. Create schema and route.
2. Implement aggregation state and instrumentation.
3. Update OpenAPI and regenerate SDK.
4. Add backend tests for the new contract.
5. Rework observability routing and navigation.
6. Rebuild the ADE Health page against the new snapshot.
7. Add dashboard tests and parity gates.
8. Remove or correct any legacy misleading code paths.

## Explicit Do-Not-Do List

- do not patch the current page by simply adding more optional fields to a handwritten frontend type
- do not expand `/api/health` into an unbounded operator payload
- do not ship with hardcoded component states
- do not leave `/observability/ade` orphaned
- do not accept manual SDK types that disagree with OpenAPI
