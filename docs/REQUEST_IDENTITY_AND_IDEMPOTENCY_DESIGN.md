# Request Identity And Idempotency Design

## Status

- Author: Codex
- Date: 2026-03-07
- Scope: SDK, dashboard, CLI, service worker, gateway middleware, write handlers, audit trail
- Motivation: establish the transport and provenance layer needed to make logical writes replay-safe, review-bound, and diagnosable across retries, reconnects, crashes, and repair flows

## Executive Summary

The repo has a real gateway-centered architecture, but approved write paths still have a correctness gap: the system cannot reliably bind one human or system action to one canonical mutation across retries, reconnects, or repair flows. Today the gateway can generate `X-Request-ID`, but that only identifies one HTTP attempt. It does not identify the reviewed action that caused the write, and it does not let the gateway distinguish retry-after-commit from stale, conflicting, or duplicated intent.

This becomes a real correctness problem because the codebase already contains retry and replay paths:

- the CLI retries transient failures for `POST`, `PATCH`, and `DELETE`
- the SDK reconnects WebSockets and supports replay cursors
- the dashboard service worker has a partial offline replay path
- some domain writes are conflict-safe, but not replay-safe

This document proposes a single end-to-end operation envelope with three distinct IDs:

1. `X-Request-ID`: one ID per HTTP attempt
2. `X-Ghost-Operation-ID`: one ID per logical user or system action
3. `Idempotency-Key`: one dedupe key per replayable write

In phase 1, the SDK and CLI will attach these headers to all mutating routes. The gateway will persist an idempotency record and either execute once or replay a previously committed response. Audit and observability will record both the request attempt and the logical operation.

This does not replace domain version checks or proposal lineage rules. It creates the transport and provenance layer that makes those domain guarantees possible to enforce and possible to debug. The important framing is that this is not primarily an observability feature. It is a write-correctness feature.

## Problem Statement

Current behavior is good enough for happy-path traffic, but weak under retry, timeout, crash, reconnect, or operator tooling.

The main failures are:

- duplicate writes can occur through approved paths
- a retried mutation cannot be distinguished from a stale or conflicting mutation
- a human decision is not yet durably bound to the reviewed proposal or revision
- logs can correlate an HTTP transaction, but not the end-to-end user action
- repair and incident tooling cannot safely answer "did this write already commit?"

This document focuses on one slice of a broader integrity problem:

- operation identity and idempotency solve replay ambiguity
- expected-version and reviewed-revision checks solve stale-decision ambiguity
- canonical proposal lineage in storage solves supersession ambiguity

Those three pieces should be treated as one program, even though this document specifies only the first layer in detail.

The design target is:

- one logical mutation executes once
- duplicate attempts return the original committed result
- misuse of the same idempotency key with a different payload fails loudly
- every write can be traced from client action to gateway commit to storage side effects

## Current State Audit

### What exists today

- The gateway injects `X-Request-ID` in middleware and returns it in the response.
- The gateway centralizes routing and RBAC in one place.
- Some domain handlers, such as goal approval, reject the most obvious duplicate transition by checking current state in SQL.
- WebSocket replay and resync logic exists for event delivery.

### What is missing today

- The SDK does not generate or send a stable logical operation ID.
- The SDK does not send an idempotency key on writes.
- The CLI retries mutating requests without any dedupe material.
- The service worker replay path does not preserve a real operation identity.
- durable local state is not yet clearly bound to a session or identity epoch
- Audit tables do not store operation identity.
- The gateway has no generic idempotency store or replay mechanism.
- CORS allows `x-request-id` as a request header, but the API does not expose the request ID or future operation headers back to browser clients.

### Concrete repo observations

- Browser SDK writes call `request(method, path, body)` with only `Authorization` and `Content-Type`.
- CLI retries `POST`, `PATCH`, and `DELETE` on `429`, `502`, `503`, and `504`.
- Goal approvals and rejections are implemented as direct state updates guarded only by `resolved_at IS NULL`.
- The dashboard service worker has a partial write replay path and currently adds `X-Ghost-Expected-Seq`, but there is no end-to-end operation identity.
- Audit writes capture actor and timestamp in some paths, but not request attempt ID, logical operation ID, idempotency key, or replay status.

## Scope Inventory

### Phase 1 mandatory endpoints

These routes must carry operation identity and idempotency from day one because they mutate durable state or cause operator-visible side effects:

- `/api/goals/:id/approve`
- `/api/goals/:id/reject`
- `/api/memory`
- `/api/memory/:id/archive`
- `/api/memory/:id/unarchive`
- `/api/workflows`
- `/api/workflows/:id`
- `/api/workflows/:id/execute`
- `/api/workflows/:id/resume/:execution_id`
- `/api/studio/sessions`
- `/api/studio/sessions/:id`
- `/api/studio/sessions/:id/messages`
- `/api/studio/sessions/:id/messages/stream`
- `/api/profiles`
- `/api/profiles/:name`
- `/api/channels`
- `/api/channels/:id`
- `/api/channels/:id/reconnect`
- `/api/webhooks`
- `/api/webhooks/:id`
- `/api/admin/backup`
- `/api/admin/provider-keys`
- `/api/admin/provider-keys/:env_name`
- `/api/pc-control/status`
- `/api/pc-control/allowed-apps`
- `/api/pc-control/blocked-hotkeys`
- `/api/pc-control/safe-zones`
- `/api/safety/pause/:agent_id`
- `/api/safety/resume/:agent_id`
- `/api/safety/quarantine/:agent_id`
- `/api/safety/kill-all`

### Phase 2 mandatory endpoints

These are also write endpoints, but can follow after the core stack is proven:

- marketplace contract and review transitions
- A2A task submission
- OAuth connect, disconnect, execute
- push subscribe and unsubscribe
- skills install and uninstall
- agent create and delete
- session bookmark and branch operations
- admin restore verification flow

### Out of scope for phase 1

- GET endpoints
- WebSocket transport itself
- domain lineage or expected-version rules beyond headers and storage hooks
- replacing existing audit/event hash chain mechanisms

## Design Principles

1. Separate request attempt identity from logical operation identity.
2. Make replay safety server-owned, not client-guessed.
3. Bind idempotency to actor plus request fingerprint.
4. Preserve exact operation identity across retries, reconnects, and offline replay.
5. Return the original committed result for duplicate attempts.
6. Fail loudly on ambiguous or unsafe reuse.
7. Make the design incremental so existing handlers can be migrated without a flag day rewrite.

## Proposed Contract

### Headers

All mutating HTTP requests will support these headers:

- `X-Request-ID`
  - per HTTP attempt
  - regenerated for each retry
  - used for transport-level tracing

- `X-Ghost-Operation-ID`
  - stable across retries for one logical action
  - UUIDv7 string
  - used for audit, provenance, and correlation

- `Idempotency-Key`
  - stable across retries for one replayable write
  - initially equal to the operation ID for SDK and CLI clients
  - used by the gateway dedupe store

- `X-Ghost-Client-ID`
  - optional in phase 1
  - stable install or device identifier
  - used for incident correlation and future device-epoch invalidation

- `X-Ghost-Session-Epoch`
  - optional in phase 2
  - increments on auth reset or session boundary invalidation
  - used by offline replay and restore flows

### Response headers

The gateway will return:

- `X-Request-ID`
- `X-Ghost-Operation-ID`
- `Idempotency-Key`
- `X-Ghost-Idempotency-Status`
  - `executed`
  - `replayed`
  - `in_progress`
  - `mismatch`

### Browser compatibility

The CORS layer must:

- allow request headers:
  - `x-request-id`
  - `x-ghost-operation-id`
  - `idempotency-key`
  - `x-ghost-client-id`
  - `x-ghost-session-epoch`
- expose response headers:
  - `x-request-id`
  - `x-ghost-operation-id`
  - `idempotency-key`
  - `x-ghost-idempotency-status`

## SDK Design

### New request options

Add a request options object:

```ts
export interface GhostRequestOptions {
  requestId?: string;
  operationId?: string;
  idempotencyKey?: string;
  idempotency?: 'required' | 'optional' | 'disabled';
}
```

Update the internal request signature to:

```ts
type GhostRequestFn = <T>(
  method: string,
  path: string,
  body?: unknown,
  options?: GhostRequestOptions,
) => Promise<T>;
```

### Default SDK behavior

- `GET`: no operation ID by default
- mutating methods:
  - create a fresh `operationId` if one is not supplied
  - set `Idempotency-Key = operationId` unless explicitly disabled
  - generate a fresh `requestId` per attempt

### Public API shape

For mutating methods, add an optional final `options` parameter:

```ts
await client.goals.approve(id, { operationId });
await client.workflows.create(params, { operationId });
```

This keeps call sites clean while allowing higher-level flows to preserve identity across retries or resume actions.

### Higher-level client helpers

Add a helper:

```ts
const op = GhostOperation.create();
await client.goals.approve(id, op.requestOptions());
```

This becomes the standard mechanism for:

- retry buttons
- offline queues
- background jobs
- multi-step UI flows

## CLI Design

The CLI is currently the highest-risk replay path because it already retries mutating requests.

### Required change

`GhostHttpClient::send_with_retry()` must:

- create one logical `operation_id` and `idempotency_key` per command invocation
- create a new `request_id` for each retry attempt
- preserve operation headers across retries

### Example

One `ghost profile update ...` command gets:

- `operation_id = 0195...`
- `idempotency_key = 0195...`
- attempt 1 `request_id = A`
- attempt 2 `request_id = B`
- attempt 3 `request_id = C`

All three attempts represent one logical mutation.

## Service Worker Design

The service worker must persist and replay the exact operation envelope.

### Required changes

Queued `PendingAction` rows must include:

- `request_id`
- `operation_id`
- `idempotency_key`
- required `session_epoch`
- required `client_id`

On replay:

- preserve `operation_id`
- preserve `idempotency_key`
- generate a fresh `request_id`

If auth resets or session epoch changes, the queue must be invalidated instead of guessed back into correctness.

If the product allows account switching on one desktop install, queued work must also be invalidated when the actor identity changes, even if auth tokens are still present locally.

## Gateway Design

### New middleware: operation context

Add middleware that runs after auth and before handlers for mutating requests.

Responsibilities:

- parse and validate operation headers
- require operation identity on configured routes
- normalize IDs into a typed `OperationContext`
- inject `OperationContext` into request extensions

Proposed type:

```rust
pub struct OperationContext {
    pub request_id: String,
    pub operation_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub actor_id: Option<String>,
    pub actor_role: Option<String>,
    pub client_id: Option<String>,
    pub session_epoch: Option<String>,
    pub method: String,
    pub path_template: String,
}
```

### New storage table

Add a table, named `operation_journal`:

```sql
CREATE TABLE operation_journal (
    idempotency_key      TEXT PRIMARY KEY,
    operation_id         TEXT NOT NULL,
    actor_id             TEXT,
    actor_role           TEXT,
    client_id            TEXT,
    session_epoch        TEXT,
    method               TEXT NOT NULL,
    path_template        TEXT NOT NULL,
    request_fingerprint  TEXT NOT NULL,
    first_request_id     TEXT NOT NULL,
    last_request_id      TEXT NOT NULL,
    status               TEXT NOT NULL,
    http_status          INTEGER,
    response_headers     TEXT,
    response_body        BLOB,
    resource_type        TEXT,
    resource_id          TEXT,
    started_at           TEXT NOT NULL,
    completed_at         TEXT,
    lease_expires_at     TEXT
);

CREATE UNIQUE INDEX idx_operation_journal_operation_id
    ON operation_journal(operation_id);
```

### Request fingerprint

The fingerprint must be computed from:

- normalized method
- route template, not raw URL
- actor identity
- stable canonical JSON body
- selected route parameters

This prevents a caller from reusing the same idempotency key for a different logical write.

### Server behavior

For configured idempotent write routes:

1. If there is no record, insert `status = in_progress` and continue.
2. If a record exists with the same fingerprint and `status = committed`, return the stored response and mark the request as `replayed`.
3. If a record exists with the same fingerprint and `status = in_progress`, return `409 Conflict` with code `OPERATION_IN_PROGRESS`, unless the lease is stale.
4. If a record exists with a different fingerprint, return `409 Conflict` with code `IDEMPOTENCY_KEY_REUSE_MISMATCH`.
5. On successful commit, persist the final HTTP status and body.

### Crash recovery

Use a lease on `in_progress` rows:

- set `lease_expires_at = now + 30s`
- refresh lease while long-running handlers stream or execute
- if a retry sees an expired lease, it may take over execution

This avoids permanent wedging after process death.

### Handler integration

Do not rewrite every handler by hand first.

Introduce a small gateway helper:

```rust
async fn execute_idempotent_json<T>(
    state: &AppState,
    op: &OperationContext,
    fingerprint: RequestFingerprint,
    f: impl Future<Output = ApiResult<T>>,
) -> ApiResult<T>
```

This helper:

- acquires or replays the operation journal row
- executes the handler body
- stores the final result
- adds response headers

## Audit And Provenance Design

### Audit requirements

Every state-changing action should be traceable by:

- request attempt ID
- operation ID
- actor
- route
- replay status
- resource touched

### Minimal schema changes

Extend `audit_log` with nullable columns:

- `request_id`
- `operation_id`
- `idempotency_key`
- `replay_status`

This is intentionally additive and backward-compatible.

### Logging rules

For every committed mutation, log:

- actor
- route
- request ID
- operation ID
- idempotency key
- replay status
- prior and new resource versions when available

This makes incident response answerable without correlating raw transport logs by hand.

## Endpoint Policy

### Policy A: Required idempotency

Use for durable writes and operator actions.

Examples:

- create, update, delete resource
- approval and rejection
- safety actions
- profile, webhook, provider key, PC control mutations
- workflow creation and execution
- studio message send

For decision-style routes such as approve or reject, this policy must eventually be paired with expected-state and reviewed-revision enforcement. Idempotency alone prevents duplicate execution. It does not prove the decision was made against the correct object version.

### Policy B: Optional idempotency

Use where the route is mutating but the immediate blast radius is lower or the route is still being migrated.

Examples:

- login
- token refresh
- push subscribe and unsubscribe

### Policy C: No idempotency

Use for:

- GET
- WebSocket upgrade
- health/readiness

## Rollout Plan

### Phase 0: Plumbing

- add request options to the SDK
- add operation headers to CLI
- add CORS allow and expose headers
- add gateway operation context middleware

Exit criteria:

- no route behavior changes yet
- headers are visible in logs and responses

### Phase 1: Journal And Replay

- add `operation_journal` migration
- implement idempotent execution helper
- migrate highest-risk routes:
  - goals
  - safety
  - provider keys
  - profiles
  - webhooks
  - workflows
  - studio messages and sessions

Exit criteria:

- duplicate retry after commit returns original response
- same key plus different payload returns `409`

### Phase 2: Adoption

- migrate remaining operator and admin write routes
- extend service worker queue shape
- make CLI preserve operation identity across retries
- store operation identity in audit logs
- bind durable local queues and caches to actor plus session epoch

Exit criteria:

- all SDK and CLI writes carry operation identity
- audit queries can search by operation ID
- stale queued writes are rejected after logout, auth reset, or account switch

### Phase 3: Make Mandatory

- reject missing operation identity on policy A routes
- add dashboards and alerts for journal mismatch and replay rates
- add runbook guidance for operators

### Phase 4: Expected-Version Integration

Once the transport layer is stable, add domain concurrency rules:

- `If-Match` or `X-Ghost-Expected-Version`
- resource version tokens on mutable resources
- lineage binding for goal and proposal transitions

This is the right sequencing. Do not try to solve domain versioning before the operation identity layer exists.

## Migration Strategy

This design must not break old clients immediately.

### Compatibility behavior

- until phase 3, missing `X-Ghost-Operation-ID` on policy A routes is allowed but logged
- the gateway will generate a synthetic operation ID for legacy clients
- synthetic IDs are marked `legacy_generated = true` in the journal

### Why

This allows phased rollout across:

- browser dashboard
- Tauri desktop
- CLI
- background tasks

## Error Model

### `409 IDEMPOTENCY_KEY_REUSE_MISMATCH`

Same idempotency key used with different fingerprint.

### `409 OPERATION_IN_PROGRESS`

Another request with the same key is still executing and the lease is not stale.

### `200/201/202 with X-Ghost-Idempotency-Status: replayed`

The gateway did not execute the mutation again. It returned the original committed result.

## Test Plan

### Unit tests

- SDK generates one operation ID per logical write
- SDK generates a new request ID per attempt
- CLI preserves operation ID across retries
- gateway fingerprint mismatch returns `409`
- gateway replay returns stored response

### Integration tests

- send the same write 100 times with the same key and confirm one commit
- simulate commit-before-response timeout, then retry
- replay queued service worker action after reconnect
- retry `ghost` CLI writes on `503` and confirm single mutation
- verify response headers are readable from browser clients under CORS

### Crash tests

- crash after durable commit but before HTTP response
- retry after process restart
- verify journal row is replayed, not re-executed

### Observability tests

- query audit log by operation ID
- correlate one operation across multiple request attempts

## Risks And Tradeoffs

### Storing response bodies

Pros:

- exact replay behavior
- simpler client semantics

Cons:

- storage growth
- potential sensitive payload retention

Mitigation:

- cap stored body size
- store only JSON responses for phase 1
- allow redaction policy per route

### Long-running streamed routes

Streaming routes are harder because the result is not a small JSON payload.

Phase 1 approach:

- treat stream start as the idempotent resource creation point
- return the same accepted metadata on replay
- do not attempt to replay the byte stream itself

### Legacy clients

Allowing synthetic operation IDs keeps compatibility, but it weakens guarantees until all clients adopt the protocol.

That is acceptable for rollout, but phase 3 must make the contract mandatory for policy A routes.

### Not sufficient by itself

This design reduces duplicate and ambiguous writes, but it does not by itself fix:

- stale approvals against superseded proposals
- missing parent-revision or expected-state checks
- storage that forgets supersession lineage after restart

Those require a follow-on state-machine and storage design. This document should be implemented as the first layer of that broader integrity program, not mistaken for the entire fix.

## Recommended Implementation Order

1. Add SDK request options and client-side ID generation.
2. Add CLI operation ID preservation across retries.
3. Add gateway operation middleware and CORS header updates.
4. Add `operation_journal` migration and helper.
5. Migrate `goals`, `safety`, `profiles`, `provider-keys`, `webhooks`, `workflows`, and `studio`.
6. Add audit log columns and provenance writes.
7. Migrate service worker queue format.
8. Make operation identity mandatory on policy A routes.
9. Add expected-version semantics as the next design.

## Bottom Line

The repo does not need a rewrite. It needs a missing systems layer.

Right now the architecture has a gateway, RBAC, storage protections, and some conflict guards. What it lacks is the operation envelope that turns retries and reconnects from "best effort" into "provably one logical write."

This design adds that layer in a way that fits the current architecture instead of fighting it. It should be implemented together with a follow-on design for reviewed-revision checks and canonical proposal lineage so the full decision path becomes replay-safe, stale-safe, and storage-canonical.
