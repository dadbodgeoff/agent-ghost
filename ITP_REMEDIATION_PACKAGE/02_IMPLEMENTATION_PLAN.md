# ITP Events Implementation Plan

Status: March 11, 2026

Purpose: translate the master spec into executable engineering work with explicit sequencing, file touch points, dependencies, and acceptance criteria.

## Execution Order

The work must proceed in this order:

1. Contract correction and ownership freeze
2. Producer-path unification
3. Gateway ingest and read-model correction
4. Live transport integration
5. Dashboard route rebuild
6. Test and parity hardening
7. Release readiness verification

Do not start with dashboard polish. The dashboard is downstream of the contract.

## Workstream 1. Contract Correction And Ownership Freeze

### Objective

Replace the misleading `/api/itp/events` semantics with a written canonical read contract before implementation branches further.

### Files Likely Touched

- `crates/ghost-gateway/src/api/itp.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/itp.ts`
- `packages/sdk/src/__tests__/client.test.ts`

### Required Changes

- Define the canonical list row shape.
- Rename misleading counters and booleans.
- Separate:
  - persisted total
  - filtered total
  - ingest backlog or buffered count
  - monitor connection health
  - extension connection health
- Stop hardcoding `platform: "gateway"` unless the event really is gateway-originated.
- Stop using `sender` as a substitute for `source`.

### Acceptance Criteria

- Every field in the response has a one-sentence semantic definition in code comments or contract notes.
- OpenAPI matches the implementation.
- SDK types are generated or aligned from the same contract.
- No test uses the old misleading names.

## Workstream 2. Producer-Path Unification

### Objective

Collapse extension-side duplication so there is one maintained event-capture path.

### Files Likely Touched

- `extension/src/content/observer.ts`
- `extension/src/background/service-worker.ts`
- `extension/src/background/itp-emitter.ts`
- `extension/src/background/gateway-client.ts`
- `extension/src/content/observer.js`
- `extension/src/background/service-worker.js`
- `extension/src/background/itp-emitter.js`
- `extension/scripts/bundle.js`
- `extension/package.json`
- `extension/manifest.chrome.json`
- `extension/manifest.firefox.json`

### Required Changes

- Choose one maintained source path, preferably the TS path.
- Remove or clearly mark the parallel JS path as generated output only.
- Standardize field naming and message types.
- Define explicit local fallback semantics.
- If extension-originated events are intended to reach the gateway, implement that as a first-class, typed, authenticated path.
- If extension local buffering exists, define flush, retry, and operator visibility.

### Acceptance Criteria

- One source implementation exists for capture/runtime logic.
- Build output cannot silently diverge from the maintained source.
- Event shape from extension is documented and typed.
- A failing native host or offline gateway has defined behavior, not implicit behavior.

## Workstream 3. Gateway Ingest And Read-Model Correction

### Objective

Make gateway-side ITP ingest and reading consistent, durable, and truthful.

### Files Likely Touched

- `crates/ghost-gateway/src/api/itp.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/cortex/cortex-storage/src/queries/itp_event_queries.rs`
- `crates/cortex/cortex-storage/src/migrations/`

### Required Changes

- Define canonical source classification for rows.
- Define platform derivation rules.
- Ensure the ingest path populates durable row fields consistently.
- Add query support needed by the ITP route:
  - pagination
  - filtering
  - ordering
  - drilldown identifiers
- If additional columns are required for truthful semantics, add them via forward migration.
- Reuse session detail/replay as the durable deep-inspection surface instead of creating a parallel weak detail API unless a new ITP-specific detail API is truly necessary.

### Acceptance Criteria

- No list row field is synthetic unless explicitly labeled as derived.
- Filtering semantics are deterministic and documented.
- Session drilldown from the global route is always possible for rows that represent session-owned events.
- Storage schema and query layer expose enough truth to support the dashboard honestly.

## Workstream 4. Live Transport Integration

### Objective

Make ITP activity first-class in ADE live transport.

### Files Likely Touched

- `crates/ghost-gateway/src/api/websocket.rs`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/routes/itp/+page.svelte`

### Required Changes

- Decide whether to:
  - introduce a first-class `ItpEvent` websocket event, or
  - formally reuse `SessionEvent` plus targeted REST refetch strategy
- Define the client behavior for:
  - new event reception
  - topic scoping
  - reconnect gap
  - `Resync`
- Ensure the route updates in real time or re-fetches in bounded, deterministic ways.

### Acceptance Criteria

- The ITP route reflects durable new events without manual refresh.
- A reconnect gap causes a full safe refresh path.
- The route cannot remain permanently stale after WS reconnect.
- SDK and dashboard agree on the websocket event taxonomy.

## Workstream 5. Dashboard Route Rebuild

### Objective

Turn the current snapshot log into a real ADE event-explorer surface.

### Files Likely Touched

- `dashboard/src/routes/itp/+page.svelte`
- optional new components under `dashboard/src/components/`
- optional new route helpers or stores under `dashboard/src/lib/`

### Required Changes

- Replace the current flat log with a structured explorer.
- Add truthful status presentation.
- Add filters.
- Add row click-through to session detail or replay.
- Expose event metadata that operators can actually act on.
- Handle loading, empty, degraded, resync, and stale states.

### Minimum UX Elements

- header with live status
- event table or list
- filter controls
- row metadata
- session link
- explicit refresh action as secondary recovery path
- explicit stale/degraded banner if live transport is not healthy

### Acceptance Criteria

- The route is useful without opening devtools.
- A user can identify event origin and navigate to durable detail from any relevant row.
- The route is consistent with existing ADE visual and interaction patterns.
- The route never labels a health signal with the wrong subsystem name.

## Workstream 6. Test And Parity Hardening

### Objective

Prevent this surface from regressing back into semantic drift.

### Files Likely Touched

- `packages/sdk/src/__tests__/client.test.ts`
- `packages/sdk/src/__tests__/websocket.test.ts`
- `dashboard/scripts/live_knowledge_audit.mjs`
- `dashboard/scripts/live_convergence_audit.mjs`
- `crates/ghost-gateway/tests/`
- optional parity or contract check scripts

### Required Changes

- Add API contract tests for real field semantics.
- Add websocket tests for live update and resync behavior.
- Add integration tests for ingest-to-persist-to-read flow.
- Add dashboard/browser tests for:
  - initial render
  - live update
  - session drilldown
  - degraded or stale state

### Acceptance Criteria

- Tests fail if old misleading field names or semantics return.
- Tests fail if the route loses live refresh behavior.
- Tests fail if drilldown is broken.
- Tests fail if SDK wrapper drifts from generated types or actual gateway payloads.

## Workstream 7. Release Readiness

### Objective

Prove the feature is shippable as a cohesive ADE subsystem.

### Required Outputs

- contract diff summary
- migration summary if schema changed
- producer path summary
- verification evidence
- residual risk list

### Acceptance Criteria

- All release gates in `03_VERIFICATION_AND_RELEASE_GATES.md` pass.
- There are no unowned semantic TODOs.
- The final implementation can be explained in one short architecture paragraph without lying.

## Dependency Notes

- Workstream 5 depends on Workstreams 1, 3, and 4.
- Workstream 4 depends on Workstream 1.
- Workstream 3 depends on Workstreams 1 and 2 if extension-originated events are intended to become first-class gateway truth.
- Workstream 6 depends on all prior workstreams.

## Explicit Anti-Patterns

The implementing agent must not do any of the following:

- patch only the dashboard labels while keeping false backend semantics
- add polling and call the route “live”
- keep both TS and JS extension sources as equal critical-path implementations
- invent new status booleans whose owner and meaning are not defined
- create an ITP detail surface that forks from the already stronger session detail/replay surfaces without strong justification
- add tests that only check route presence or heading visibility
