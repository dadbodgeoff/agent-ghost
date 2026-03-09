# RBAC/Authz Remediation Tasks

Status: Completed on March 9, 2026

Objective: move GHOST authorization from a fragmented role-string model to one typed, fail-closed, action-based authorization system with explicit capability support, versioned privilege claims, shadow-mode cutover, and exhaustive negative testing.

Authoritative design: `RBAC_AUTHZ_REMEDIATION_DESIGN.md`

Implementation companion: `RBAC_AUTHZ_REMEDIATION_IMPLEMENTATION.md`

This file is the execution tracker for the authz remediation. If this file conflicts with the design on privilege semantics, the design wins.

## Engineering Standard

This work is held to the following bar:

- No raw role-string authorization in production handlers.
- No hidden privilege widening inside compatibility code.
- No route-level auth claims that are not backed by typed policy.
- No capability without a narrow, written scope.
- No authorization change without explicit deny-path tests.
- No route/handler mismatch that can compile silently.
- No reliance on comments as the only source of privilege truth.
- No cutover without shadow evidence and rollback readiness.

## Confirmed Gaps

1. Authorization truth is split across claims parsing, route grouping, and handlers.
2. `security_reviewer` exists in safety semantics but not in the shared RBAC model.
3. The current model assumes hierarchy where the product now needs orthogonal privilege.
4. Unknown privilege state handling is underspecified.
5. OpenAPI/comments can drift away from route and handler behavior.
6. The original authz docs were too weak on context, claims schema, and cutover mechanics.

## Required Artifacts

The following artifacts are mandatory outputs of this remediation:

1. `docs/authz/AUTHZ_ACTION_INVENTORY.md`
2. `docs/authz/AUTHZ_CLAIMS_SCHEMA.md`
3. `docs/authz/AUTHZ_ROUTE_ACTION_MATRIX.md`
4. `docs/authz/AUTHZ_SHADOW_DIFF_REPORT.md`
5. `docs/authz/AUTHZ_COMPAT_DEPRECATION_CHECKLIST.md`

## Phase Gates

The work may not advance past a phase gate until the gate criteria are met.

1. Model Freeze Gate
   - action inventory exists
   - claims schema exists
   - `security_reviewer` representation is explicitly approved

2. Shadow Gate
   - legacy and new authorizers both run
   - mismatches are logged and classified
   - no unexplained mismatch remains open for cutover candidates

3. Enforced Gate
   - matrix tests pass
   - route/action matrix is complete
   - rollback path is verified

4. Legacy Deletion Gate
   - enforced mode is stable
   - shadow report is closed
   - deprecation checklist is complete

## Workstreams

## Workstream A: Model Freeze and Artifacts

Goal: define the final authz model before implementation sprawl resumes.

Tasks:

1. Approve the base role model:
   - `viewer`
   - `operator`
   - `admin`
   - `superadmin`

2. Approve the capability model:
   - initial capability set
   - capability naming rules
   - capability ownership rules

3. Approve the distinction between:
   - authorization
   - business-state preconditions

4. Produce:
   - action inventory artifact
   - claims schema artifact

Acceptance criteria:

- the design doc is accepted as the source of truth
- no unresolved disagreement remains on whether `security_reviewer` is a role or capability
- every privileged route appears in the action inventory
- the claim wire format is explicit rather than deferred

## Workstream B: Typed Authz Core

Goal: introduce typed authz primitives without changing product behavior more than necessary.

Tasks:

1. Add `Principal`, `BaseRole`, `Capability`, `AuthMode`, `Action`, and `AuthorizationContext`.
2. Add a single claims-to-principal normalization path.
3. Add typed policy predicates.
4. Ensure malformed privilege input fails closed.

Acceptance criteria:

- all authz code can depend on typed primitives instead of raw claim strings
- resource-aware authorization is possible without new handler-local conventions

## Workstream C: Policy Registry and Dual Authorizer

Goal: create one authorization owner for privileged operations and retain the old authorizer for diffing.

Tasks:

1. Create the action registry.
2. Define one policy rule per privileged action.
3. Add a shared authorizer API used by middleware and handlers.
4. Add the legacy wrapper authorizer for diffing.

Acceptance criteria:

- privileged decisions can be expressed without ad hoc role parsing
- legacy and new decisions can be compared for the same request

## Workstream D: Shadow Mode and Diff Reporting

Goal: validate the new model before enforcement.

Tasks:

1. Add enforcement modes:
   - `LegacyOnly`
   - `Shadow`
   - `Enforced`
2. Run both authorizers in `Shadow` mode.
3. Record mismatches by route, action, and principal summary.
4. Produce and maintain the shadow diff report artifact.

Acceptance criteria:

- mismatches are observable, classified, and reviewable
- no enforced cutover proceeds without shadow evidence

## Workstream E: Route Binding and Handler Seam Removal

Goal: remove production handler-local role-string checks and eliminate route/action drift.

Tasks:

1. Bind non-public routes to explicit route/action metadata.
2. Migrate safety handlers.
3. Migrate admin and provider-key handlers.
4. Migrate any remaining privileged handler seams found by search.
5. Produce the route/action matrix artifact.

Acceptance criteria:

- no production authz path depends on `claims.role == ...`
- no non-public route is mounted without action metadata
- route/action matrix is complete

## Workstream F: Capability Rollout

Goal: implement `security_reviewer` correctly.

Tasks:

1. Add capability representation to normalized principal state.
2. Define reviewer-allowed actions.
3. Keep reviewer privilege narrow.
4. Define who may mint capability-bearing tokens.

Acceptance criteria:

- safety reviewer privilege exists without implying admin privilege
- capability-bearing claims are explicitly governed

## Workstream G: Route and Docs Alignment

Goal: make route placement, policy, and docs agree.

Tasks:

1. Audit route groups against action policy.
2. Move mismatched routes.
3. Align OpenAPI and comments with real policy.

Acceptance criteria:

- route grouping no longer contradicts handler semantics
- public docs do not overstate or understate real privilege behavior

## Workstream H: Enforced Cutover and Verification

Goal: prove the model under denial pressure and move to enforcement deliberately.

Tasks:

1. Add matrix tests for every privileged action.
2. Add route/handler parity tests.
3. Add compatibility tests for legacy and no-auth modes.
4. Add malformed-claim tests.
5. Add cutover and rollback tests.
6. Move from `Shadow` to `Enforced` only after the enforced gate is met.

Acceptance criteria:

- every privileged action has explicit allow and deny coverage
- rollback path is verified under test
- enforced mode is entered deliberately, not implicitly

## Workstream I: Cleanup and Deletion

Goal: remove obsolete authz logic after the new model is live.

Tasks:

1. Delete compatibility-only string helpers no longer needed.
2. Remove stale comments and duplicated local auth helpers.
3. Document final extension points for future capabilities.
4. Close the shadow diff report.
5. Complete the compatibility deprecation checklist.

Acceptance criteria:

- the final authz surface is smaller, not just more layered
- legacy deletion happens only after the legacy deletion gate is satisfied

## Phase Order

1. Workstream A
2. Workstream B
3. Workstream C
4. Workstream D
5. Workstream E
6. Workstream F
7. Workstream G
8. Workstream H
9. Workstream I

## Exit Criteria

This task is complete only when:

- the typed authz model is the only real authz model
- `security_reviewer` is implemented as designed
- route/middleware/handler/docs privilege rules agree
- matrix tests prove deny behavior across all privileged actions
- no raw handler-level role-string authz remains in production code
- the action inventory, claims schema, route/action matrix, and shadow diff artifacts exist and are complete
- all of the above are now true for the current gateway authz surface
- cutover and rollback behavior have both been proven
