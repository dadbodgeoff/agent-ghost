# ADE Observability Current-State Audit

Date: March 11, 2026

Status: Open

Scope:
- dashboard observability surfaces
- ADE self-observability route
- gateway and monitor telemetry surfaces
- SDK and OpenAPI contract authority
- validation coverage required to stop regression

## Executive Judgment

ADE observability is not currently a cohesive subsystem.

The repo contains two separate observability experiences:
- `/observability` for trace waterfall inspection
- `/observability/ade` for ADE self-observability

The self-observability page is not authoritative, not fully wired, and not navigationally integrated. Several metrics are placeholders, several statuses are inferred incorrectly, and the public contract is not strong enough to prevent future drift.

## Critical Findings

### 1. The ADE page reads fields that the gateway does not return

The page expects:
- `uptime_secs`
- `db_size_bytes`
- `active_agents`
- `active_ws_connections`

The current gateway `/api/health` handler does not emit those fields.

Impact:
- the UI cannot present truthful ADE metrics
- empty placeholders are treated as if the page is merely waiting on data, when the contract does not exist
- future readers are misled into thinking the system already exports these metrics

## 2. Health semantics are flattened into a false green state

The ADE page derives most component status from `health.status === "alive"`.

The gateway intentionally returns `"alive"` for:
- `Healthy`
- `Degraded`
- `Recovering`

Impact:
- degraded and recovering states appear healthy
- convergence monitor truth is lost
- operators are shown operational optimism instead of actual system state

## 3. The self-observability page is orphaned from the ADE information architecture

The main sidebar links to `/observability`, but the repo contains no explicit navigation entry to `/observability/ade` outside that page itself.

Impact:
- the page exists but is not part of the normal operator path
- the app presents observability as a single top-level concept while actually shipping disconnected surfaces
- users cannot rely on a stable mental model for observability

## 4. Multiple component rows are static assertions rather than telemetry

The ADE page currently hardcodes or effectively hardcodes:
- API status as `Reachable`
- backup scheduler as `Scheduled`
- config watcher as `Watching`
- database health as `Connected` whenever gateway liveness is OK
- websocket handler as healthy whenever the connection count is greater than or equal to zero, which is always true

Impact:
- the page is representational, not diagnostic
- failures in backup/config/websocket/database subsystems would not be surfaced truthfully
- operators cannot use the page to distinguish health from degraded operation

## 5. Real telemetry exists in parts of the system but is not composed into ADE observability

The repo already has real sources of truth for parts of the problem:
- convergence monitor uptime and status
- websocket connection tracking
- database tooling that can calculate database size
- config watcher and periodic task infrastructure

Impact:
- the missing capability is not raw observability everywhere; it is aggregation, contract design, and truthful presentation
- without a canonical aggregator, every surface will continue to invent its own local truth

## 6. Contract authority is weak enough that drift passed undetected

Current state:
- the gateway returns ad hoc JSON from `/api/health`
- OpenAPI documents `/api/health` without a typed response body
- the generated SDK types therefore do not authoritatively model the response
- the manual SDK health type already diverges from the gateway naming
- the SDK test accepts an invalid health payload shape

Impact:
- frontend/backend drift is easy to introduce
- the SDK cannot be trusted as an authority model
- tests currently protect request execution, not semantic correctness

## 7. The ADE page is snapshot-only rather than a live observability surface

The page loads once on mount and supports manual refresh. It does not subscribe to the dashboard WebSocket fabric or poll on an interval with stale-state semantics.

Impact:
- the page is not suitable for active operator use
- the user must infer whether values are current
- ADE self-observability behaves unlike the rest of the dashboard

## Required Remediation Outcome

The target system must satisfy all of the following:

1. One canonical ADE observability contract exists.
2. Every displayed status is backed by real telemetry or explicit unavailability.
3. The dashboard information architecture exposes observability as one coherent area.
4. Live/stale/degraded semantics are explicit.
5. OpenAPI, SDK, backend, and dashboard use the same contract names and shapes.
6. Regression gates fail if the contract drifts again.

## Non-Goals

This remediation is not a request to:
- redesign all dashboard pages
- replace the existing trace waterfall
- build a generalized metrics platform
- expose internal implementation details that do not support operator action

The goal is precise: make ADE observability truthful, unified, testable, and implementation-ready.
