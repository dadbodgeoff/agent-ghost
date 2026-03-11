# ADE Observability Target Architecture

Date: March 11, 2026

Status: Target-state design

## Design Intent

ADE observability must become a first-class subsystem with:
- one authority contract
- one coherent operator information architecture
- truthful runtime and degraded-state semantics
- enough validation that drift is blocked before merge

The correct approach is to keep `/api/health` as a liveness/readiness-oriented bootstrap surface and introduce a dedicated ADE observability snapshot surface.

## Canonical Public Surface

### REST

Add:
- `GET /api/observability/ade`

Response type:
- `AdeObservabilitySnapshot`

Optional later expansion:
- `GET /api/observability/ade/history`
- `GET /api/observability/ade/components/{component}`

Do not overload `/api/health` with all ADE operator detail.

Reason:
- `/api/health` is a probe surface
- ADE observability is an operator surface
- mixing probe semantics and operator semantics creates bad coupling and ambiguous contracts

## Canonical Snapshot Shape

```ts
interface AdeObservabilitySnapshot {
  sampled_at: string;
  stale: boolean;
  status: "healthy" | "degraded" | "recovering" | "unavailable";
  gateway: {
    liveness: "alive" | "unavailable";
    readiness: "ready" | "not_ready";
    state:
      | "Healthy"
      | "Degraded"
      | "Recovering"
      | "Initializing"
      | "ShuttingDown"
      | "FatalError";
    uptime_seconds: number | null;
    platform_killed: boolean;
  };
  monitor: {
    enabled: boolean;
    connected: boolean;
    status: "running" | "degraded" | "disabled" | "unreachable";
    uptime_seconds: number | null;
    agent_count: number | null;
    event_count: number | null;
    last_computation: string | null;
  };
  agents: {
    active_count: number;
    registered_count: number;
  };
  websocket: {
    active_connections: number | null;
    per_ip_limit: number | null;
    status: "healthy" | "degraded" | "unavailable";
  };
  database: {
    path: string | null;
    size_bytes: number | null;
    wal_mode: boolean | null;
    status: "healthy" | "degraded" | "unavailable";
    last_error: string | null;
  };
  backup_scheduler: {
    enabled: boolean;
    status: "healthy" | "degraded" | "disabled" | "unavailable";
    retention_days: number | null;
    schedule: string | null;
    last_success_at: string | null;
    last_failure_at: string | null;
    last_error: string | null;
  };
  config_watcher: {
    enabled: boolean;
    status: "healthy" | "degraded" | "disabled" | "unavailable";
    watched_path: string | null;
    last_reload_at: string | null;
    last_error: string | null;
  };
  autonomy: unknown;
  convergence_protection: unknown;
  distributed_kill: unknown;
  speculative_context: unknown;
}
```

## Data Ownership

### Gateway-owned fields

The gateway is the aggregator of record for:
- gateway FSM state
- readiness/liveness summary
- active/registered agent counts
- websocket connection count
- database metrics
- backup scheduler status
- config watcher status
- autonomy summary
- convergence protection summary
- distributed kill summary
- speculative context summary

### Monitor-owned fields

The monitor remains the authority for:
- monitor health status
- monitor uptime
- agent count as seen by monitor
- monitor event count
- monitor last computation timestamp

The gateway may proxy or ingest these values, but the snapshot must clearly preserve that they are monitor-derived.

## Runtime Aggregation Model

### Preferred pattern

Create a gateway-owned aggregation service that:
- maintains a small in-memory status snapshot for ADE observability
- updates snapshot fields from local subsystems
- polls the convergence monitor status endpoint on an interval
- records `sampled_at`
- marks the snapshot stale if refresh deadlines are missed

Avoid performing multiple slow subsystem queries directly inside the request handler.

### Required local instrumentation

The gateway must expose or track:
- process start time for uptime
- websocket total connection count
- database file path and size
- last database health error
- backup scheduler last success/failure/error
- config watcher last successful reload and last error

## Dashboard Information Architecture

The dashboard must expose observability as one area with explicit sub-navigation:

- `Observability`
  - `Traces`
  - `ADE Health`

Target routes:
- `/observability/traces`
- `/observability/ade`

The current `/observability` route should become either:
- a redirect to `/observability/traces`, or
- a layout route with sub-navigation and a default child

The ADE Health page must not remain an unlinked orphan.

## Dashboard Behavior

The ADE Health page must:
- render from `AdeObservabilitySnapshot`
- distinguish `healthy`, `degraded`, `recovering`, and `unavailable`
- display stale-state messaging explicitly
- subscribe to live refresh via WebSocket or poll on a bounded interval
- show actual unavailability instead of invented success
- preserve last known good snapshot only when the UI explicitly labels it stale

The page must not:
- hardcode component status text
- infer green state from liveness alone
- display "Reachable" unless backed by a real field

## SDK Authority Model

The SDK must expose:
- generated `AdeObservabilitySnapshot` types from OpenAPI
- a thin `ObservabilityAPI` wrapper with `ade()` or `getAdeSnapshot()`

The SDK must not:
- shadow the contract with handwritten incompatible names
- rely on `unknown`, `any`, or untyped JSON for this surface

## Invariants

1. No field shown in the dashboard may be absent from the authoritative public contract.
2. No component row may claim healthy unless it is derived from a real signal.
3. Degraded and recovering states must remain visible through the full stack.
4. Probe surfaces and operator surfaces must remain distinct.
5. OpenAPI is the root public authority.
6. Generated SDK types must match live server output.
7. The observability IA must make all observability surfaces discoverable.

## Definition Of Architectural Success

The architecture is complete when:
- the dashboard and SDK can be regenerated from OpenAPI without hand-patching
- the ADE Health page contains zero placeholder-health assertions
- the page is reachable from the main observability navigation
- a degraded monitor or stale snapshot is reflected truthfully end to end
