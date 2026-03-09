# RBAC/Authz Remediation Implementation

**Version**: 0.1
**Date**: March 9, 2026
**Status**: Implemented and closed on March 9, 2026
**Authoritative Design**: `RBAC_AUTHZ_REMEDIATION_DESIGN.md`
**Execution Tracker**: `RBAC_AUTHZ_REMEDIATION_TASKS.md`

---

## Summary

This document translates the RBAC/authz remediation design into concrete implementation work against the current repo.

Closeout:

- the typed authorizer is the only runtime authorization path
- non-public routes bind through `RouteAuthorizationSpec`
- live execution visibility is enforced through owner-aware typed middleware
- handler-local role-string checks were removed from the production authz path
- the legacy evaluator and enforcement-mode scaffolding were deleted after cutover

The design document owns the model.
This file owns the code changes, sequencing, and acceptance criteria.

If this file conflicts with the design on privilege semantics, the design wins.

---

## Current Code Surface

Primary files in scope:

- `crates/ghost-gateway/src/api/auth.rs`
- `crates/ghost-gateway/src/api/rbac.rs`
- `crates/ghost-gateway/src/api/safety.rs`
- `crates/ghost-gateway/src/api/admin.rs`
- `crates/ghost-gateway/src/api/provider_keys.rs`
- `crates/ghost-gateway/src/api/live_executions.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/tests/backup_operation_journal_tests.rs`

New files expected:

- `crates/ghost-gateway/src/api/authz.rs`
- `crates/ghost-gateway/src/api/authz_policy.rs`
- `crates/ghost-gateway/src/api/authz_legacy.rs`
- `crates/ghost-gateway/tests/authz_matrix_tests.rs`

Optional new files if separation improves clarity:

- `crates/ghost-gateway/src/api/authz_route.rs`

---

## Implementation Rules

- no new raw role string comparisons in production code
- no permission decision embedded only in route comments
- no direct route-level privilege exception without an `Action`
- no backward-compatibility privilege broadening
- no capability introduced without at least one deny test
- no cutover to enforced mode without shadow-mode evidence

## Required Artifacts

This remediation is not complete without these artifacts:

- `docs/authz/AUTHZ_ACTION_INVENTORY.md`
- `docs/authz/AUTHZ_CLAIMS_SCHEMA.md`
- `docs/authz/AUTHZ_ROUTE_ACTION_MATRIX.md`
- `docs/authz/AUTHZ_SHADOW_DIFF_REPORT.md`
- `docs/authz/AUTHZ_COMPAT_DEPRECATION_CHECKLIST.md`

---

## Phase 0: Freeze the Surface

Goal: stop authz drift while the refactor is in progress.

Changes:

- add a short comment in `crates/ghost-gateway/src/api/rbac.rs` and `crates/ghost-gateway/src/api/safety.rs` stating that new handler-local role string checks are forbidden
- document the authoritative workstream in the new task tracker
- create the action inventory and claims-schema artifact stubs

Acceptance criteria:

- the repo has an explicit active remediation record for authz
- contributors have one obvious place to extend instead of patch locally
- the inventory and schema artifacts exist and are the active review targets

---

## Phase 1: Freeze Inventory and Wire Contract

Goal: remove ambiguity before code churn begins.

### 1.1 Create the action inventory

Add `docs/authz/AUTHZ_ACTION_INVENTORY.md` with at least:

- route id
- method
- path
- action
- current route tier
- current handler-local checks
- target policy owner
- resource context needed

### 1.2 Create the claims schema decision

Add `docs/authz/AUTHZ_CLAIMS_SCHEMA.md` defining:

- `AuthzClaimsV1`
- migration treatment of legacy role-only JWTs
- capability encoding
- unknown claim failure behavior
- who may mint capability-bearing tokens

Acceptance criteria:

- no unresolved ambiguity remains on claim wire format
- every privileged route appears in the action inventory

---

## Phase 2: Introduce Typed Authz Core

Goal: create a typed model without changing external behavior more than necessary.

### 2.1 Create `authz.rs`

Add:

- `BaseRole`
- `Capability`
- `AuthMode`
- `Principal`
- `Action`
- `RouteId`
- `TransportKind`
- `AuthorizationContext`
- `PolicyPredicate`
- `PolicyRule`
- `AuthzEnforcementMode`
- parse/normalize helpers for claims to principal

Key requirements:

- `dev` normalizes to `BaseRole::Operator`
- unknown roles fail closed in the new path
- legacy token fallback and no-auth fallback normalize through the same type

### 2.2 Move role parsing ownership

`crates/ghost-gateway/src/api/rbac.rs` should stop owning the canonical role parser.

Instead:

- `authz.rs` owns parsing and normalization
- `rbac.rs` becomes middleware glue over the authorizer

Acceptance criteria:

- there is one canonical role parser
- `Claims` no longer need to be interpreted ad hoc across the codebase
- typed authz can describe resource-aware decisions without handler-local string checks

---

## Phase 3: Introduce Policy Registry and Legacy Wrapper

Goal: define one policy owner for privileged operations.

### 3.1 Create `authz_policy.rs`

Add:

- `PolicyRule`
- static mapping from `Action` to `PolicyRule`
- helper APIs:
  - `policy_for(action)`
  - `route_spec_for(route_id)`

Initial action inventory should cover at minimum:

- safety status
- pause agent
- quarantine agent
- resume quarantine
- kill all
- backup create
- backup list
- export data
- restore verify
- provider key read
- provider key write
- provider key delete

### 3.2 Create authorizer entry points

Add typed helpers:

- `authorize(principal, context) -> Result<AuthorizationDecision, ApiError>`
- `authorize_claims(claims, context) -> Result<(Principal, AuthorizationDecision), ApiError>`
- `legacy_authorize(claims, route_id) -> Result<AuthorizationDecision, ApiError>`

`AuthorizationDecision` should include:

- allow/deny
- matched action
- matched policy id
- denial reason class

This must return the resolved principal on success so handlers can use it for actor identity and audit metadata without reparsing.

Acceptance criteria:

- every privileged decision can be expressed as `authorize(..., context)`
- the old authorizer can be run in parallel for diffing
- no new code path needs to know how role ordering or capability parsing works internally

---

## Phase 4: Add Shadow-Mode Dual Evaluation

Goal: compare new and old authorization behavior before cutover.

### 4.1 Add enforcement mode plumbing

Introduce `AuthzEnforcementMode` resolution in bootstrap/config.

Required modes:

- `LegacyOnly`
- `Shadow`
- `Enforced`

### 4.2 Emit shadow diffs

In `Shadow` mode:

- enforce legacy decisions
- compute new authorizer decisions in parallel
- log and aggregate mismatches by:
  - route id
  - action
  - principal summary
  - mismatch class

### 4.3 Produce shadow report artifact

Write or update `docs/authz/AUTHZ_SHADOW_DIFF_REPORT.md` with:

- expected mismatches
- resolved mismatches
- unexplained mismatches

Acceptance criteria:

- the repo can run both old and new decisions side by side
- mismatches are observable and classifiable
- no cutover proceeds without shadow evidence

---

## Phase 5: Migrate Middleware

Goal: make route middleware consume the typed authorizer.

### 5.1 Refactor `rbac.rs`

Replace rank-only middleware internals with:

- principal resolution
- route-spec lookup
- action-aware authorizer call

Keep thin convenience wrappers if useful:

- `viewer`
- `operator`
- `admin`
- `superadmin`

But treat them as compatibility helpers, not the policy source.

### 5.2 Add mandatory route-action helper

Add:

- `require_route(RouteAuthorizationSpec)`

This is not optional. Non-public routes must register through an action-bearing authz helper.

Acceptance criteria:

- middleware and handlers share the same principal resolution and deny semantics
- malformed claims fail consistently before handler execution
- non-public route registration without action metadata is no longer possible

---

## Phase 6: Migrate Handler Authorization Seams

Goal: remove handler-local raw string privilege logic.

### 6.1 Safety

Refactor `crates/ghost-gateway/src/api/safety.rs` to:

- use typed principal data
- use `AuthorizationContext`
- use action-based authorization helpers
- keep forensic review, second confirmation, and expected-level checks as business-state validation

Important:

- `resume quarantine` should be expressed as an action policy plus business preconditions
- `safety status` visibility logic should use typed privilege helpers rather than string matches

### 6.2 Admin and provider keys

Refactor:

- `crates/ghost-gateway/src/api/admin.rs`
- `crates/ghost-gateway/src/api/provider_keys.rs`

to remove local auth helpers that duplicate middleware semantics.

### 6.3 Other policy seams

Audit and migrate:

- `crates/ghost-gateway/src/api/live_executions.rs`
- any remaining privileged handlers found by search

Acceptance criteria:

- no production file in `crates/ghost-gateway/src/api` compares `claims.role` directly for authorization
- all privileged handlers use typed authorization helpers
- resource-sensitive checks use `AuthorizationContext`, not ad hoc local conventions

---

## Phase 7: Introduce Capability-Based Safety Review

Goal: implement `security_reviewer` correctly.

### 7.1 Claims compatibility

Extend `Claims` or add a compatibility decoding layer so JWTs can represent:

- base role
- optional capabilities

Preferred direction:

- keep `role` for compatibility
- add optional `capabilities: Vec<String>`
- normalize both into `Principal`

### 7.2 Safety review policy

Define the exact allowed set for `Capability::SafetyReview`.

Initial recommendation:

- allows quarantine review and resume flows
- allows elevated safety detail visibility
- does not allow provider keys, backup, export, restore, or kill-all

### 7.3 Issuance behavior

Current local login issues `admin` only.

That is acceptable during early migration, but the implementation must now also define:

- whether local login can ever mint capabilities
- which issuer path may mint `Capability::SafetyReview`
- whether capability-bearing tokens require `authz_v = 1`

Acceptance criteria:

- safety reviewer semantics are real, typed, and narrow
- reviewer privilege does not accidentally imply admin privilege
- capability-bearing issuance is explicitly constrained and tested

---

## Phase 8: Route Declaration Cleanup

Goal: make route authorization legible and mechanically aligned with policy.

### 8.1 Audit route grouping

Review `crates/ghost-gateway/src/route_sets.rs` and move any mismatched routes so route grouping reflects the documented minimum tier.

### 8.2 Build route/action matrix

Populate `docs/authz/AUTHZ_ROUTE_ACTION_MATRIX.md` with:

- method
- path
- route id
- action
- minimum allow set
- resource context required
- current owner file

Acceptance criteria:

- route placement does not contradict action policy
- permission documentation is not inferred from stale group names alone
- every non-public route appears in the route/action matrix

---

## Phase 9: OpenAPI and Documentation Alignment

Goal: stop privilege docs from drifting away from code.

Changes:

- update `crates/ghost-gateway/src/api/openapi.rs`
- update endpoint comments where needed
- document how action policy maps to public privilege descriptions

Preferred outcome:

- auth requirements are derived from action policy metadata or verified against it

Acceptance criteria:

- privilege descriptions in OpenAPI match live gateway behavior
- no route advertises weaker or stronger requirements than the authorizer enforces

---

## Phase 10: Cutover, Rollback, and Verification Matrix

Goal: prove the model rather than assuming it.

### 10.1 Add `authz_matrix_tests.rs`

Include explicit allow/deny tests for all privileged actions across:

- anonymous
- malformed role
- `viewer`
- `operator`
- `admin`
- `superadmin`
- `dev`
- `operator + SafetyReview`

### 10.2 Add route/handler parity tests

Ensure:

- route middleware allows exactly the set the handler authorizer expects
- no route passes middleware only to fail the same privilege check inside the handler

### 10.3 Add compatibility tests

Cover:

- old JWT role-only claims
- capability-bearing JWT claims
- no-auth dev fallback
- legacy token fallback

### 10.4 Add cutover and rollback tests

Cover:

- `LegacyOnly` behavior
- `Shadow` mismatch emission
- `Enforced` behavior
- downgrade from `Enforced` back to `Shadow` or `LegacyOnly` during incident recovery

Acceptance criteria:

- matrix tests prove both positive and negative cases
- all known historical authz seam failures are captured as regressions
- cutover and rollback mechanics are themselves tested

---

## Exit Criteria

Implementation is complete only when:

- `authz.rs` and `authz_policy.rs` are the obvious authz owners
- `rbac.rs` is a thin adapter, not a second policy engine
- `security_reviewer` exists as typed capability logic
- no production handler performs raw role-string authz
- route grouping and action policy agree
- the authz matrix proves deny behavior across all privileged actions
- shadow-mode mismatch evidence exists and is resolved
- rollback remains possible until legacy deletion is complete

Until then, the system is in migration, not done.
