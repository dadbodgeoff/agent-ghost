# ITP Verification And Release Gates

Status: March 11, 2026

Purpose: define the minimum proof required before the rebuilt ITP capability can be treated as complete.

## Verification Standard

The burden of proof is on the implementation, not on reviewers inferring correctness from code shape.

The ITP rebuild is not complete if:

- fields are renamed but their semantics remain ambiguous
- live updates work only in the happy path
- the route still has no durable drilldown path
- extension behavior is only manually reasoned about
- SDK and gateway payloads are merely “close enough”

## Gate 1. Public Contract Truthfulness

### Required Proof

- OpenAPI matches the gateway implementation.
- Generated SDK types match the public API.
- Hand-written SDK wrappers do not fork from generated types without explicit justification.
- Every response field exposed by `/api/itp/events` has documented semantics.

### Required Tests

- gateway API unit or integration tests for `/api/itp/events`
- SDK client contract tests

## Gate 2. Producer Path Integrity

### Required Proof

- One maintained extension source path exists.
- Extension-originated event shape is deterministic.
- Native-host failure, offline buffering, and retry or local-only behavior are explicitly tested or intentionally disabled.

### Required Tests

- extension unit tests where practical
- build verification proving source-to-dist mapping is intentional

## Gate 3. Durable Persistence Integrity

### Required Proof

- New or changed ITP rows persist with truthful metadata.
- Query results can distinguish source, platform, session, and event type correctly.
- Any new schema changes are forward-only and migration-tested.

### Required Tests

- storage query tests
- gateway ingest tests
- migration tests if schema changed

## Gate 4. Live ADE Behavior

### Required Proof

- The dashboard route updates when durable ITP activity occurs.
- Reconnect gaps trigger correct refresh behavior.
- The route does not silently stay stale.

### Required Tests

- websocket tests for event delivery and resync
- dashboard integration or Playwright tests for live refresh behavior

## Gate 5. Cohesive Navigation

### Required Proof

- From the global ITP route, a user can open durable detail for a relevant event.
- Session linkage is never missing for session-owned events unless explicitly documented.
- The route does not strand users on a shallow summary screen.

### Required Tests

- browser test for row click-through to session detail or replay

## Gate 6. Operator Truthfulness

### Required Proof

- any displayed count corresponds to the exact backend value it claims to represent
- any displayed connectivity status corresponds to the subsystem named in the label
- degraded states are explicit

### Required Tests

- API contract tests for status fields
- dashboard tests for degraded banners or stale-state rendering

## Gate 7. Regression Hardening

### Required Proof

- The repo contains tests that would fail if the previous misleading semantics returned.
- The repo contains tests that would fail if the route reverted to snapshot-only without explicit contract change.

### Required Tests

- targeted regression tests for:
  - `buffer_count` semantics
  - `extension_connected` semantics
  - `platform` derivation
  - `source` derivation
  - missing websocket wiring

## Recommended Evidence Bundle

Before declaring done, the implementing agent should be able to provide:

- one concise architecture summary
- list of files changed by workstream
- output from relevant tests
- any migration note
- before/after contract summary
- residual risks, if any

## Release Blockers

The following conditions are release blockers:

- dashboard still says live while only polling or manual refresh is in use
- extension and monitor/gateway connectivity are still conflated
- `platform` or `source` fields are still guessed rather than derived truthfully
- a second extension source path remains live and semantically divergent
- no browser-level test covers the route beyond page heading visibility
