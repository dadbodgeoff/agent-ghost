# ADE Observability Validation Gates

Date: March 11, 2026

Status: Required verification plan

## Standard Of Evidence

This work is not complete when the UI looks right. It is complete when:
- the contract is typed and authoritative
- the backend returns truthful telemetry
- the dashboard renders healthy and failure cases correctly
- automated tests fail on drift

## Required Test Categories

### 1. Backend contract tests

Add tests that verify:
- `GET /api/observability/ade` returns a typed JSON body
- degraded monitor connectivity is surfaced explicitly
- absent local telemetry becomes `null` plus truthful component status, not fabricated success
- field names match the published schema exactly

Preferred location:
- `crates/ghost-gateway/tests/`

### 2. Backend subsystem truth tests

Add targeted tests for:
- websocket connection counting
- backup scheduler health bookkeeping
- config watcher health bookkeeping
- database metrics and WAL reporting
- snapshot stale-state transitions

Preferred location:
- unit tests near subsystem code
- integration tests in `crates/ghost-gateway/tests/`

### 3. OpenAPI and SDK parity checks

Required evidence:
- the OpenAPI route declares a real response body
- generated SDK types include the response body
- no manual SDK naming drift remains for this surface

The implementation should either:
- extend an existing parity script, or
- add a new strict check specific to observability

### 4. SDK tests

Add or update SDK tests so they verify:
- `client.observability.ade()` hits the correct route
- the expected response shape is preserved
- health route tests no longer allow invalid shapes like `{ status: "ok" }`

Preferred location:
- `packages/sdk/src/__tests__/client.test.ts`

### 5. Dashboard component and route tests

Add browser coverage for:
- healthy snapshot
- degraded monitor snapshot
- stale snapshot
- request failure
- navigation into both `Traces` and `ADE Health`

Preferred location:
- `dashboard/tests/observability.spec.ts`

## Manual Verification Checklist

The operator should be able to verify:

1. The sidebar path to observability is obvious and stable.
2. Traces and ADE health are both reachable within observability.
3. The ADE page shows gateway state, not just gateway liveness.
4. Disconnecting or degrading the monitor does not leave the page green.
5. The websocket subsystem can show zero connections or unavailable state without being marked healthy by default.
6. A backend request failure does not leave "API Reachable" painted on screen.
7. A stale snapshot is labeled stale.

## Release Gates

The work should not merge until all of the following pass:

- `cargo test -p ghost-gateway`
- targeted gateway integration tests for observability
- `pnpm --dir packages/sdk exec vitest run src/__tests__/client.test.ts`
- `pnpm --dir packages/sdk typecheck`
- `pnpm --dir dashboard check`
- `pnpm --dir dashboard exec playwright test tests/observability.spec.ts`

If a repo-standard parity command exists for OpenAPI freshness, it must also pass after regeneration.

## Failure Policy

Any of the following is a merge blocker:
- missing OpenAPI body for the new route
- manual SDK type that does not match generated output
- dashboard rendering that implies healthy while snapshot state is degraded
- hardcoded health strings remaining in the ADE page without backing fields
- navigation that still leaves ADE health unreachable from the main observability path
