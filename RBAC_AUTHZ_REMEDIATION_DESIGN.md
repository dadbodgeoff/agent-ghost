# RBAC/Authz Remediation Design

**Version**: 0.1
**Date**: March 9, 2026
**Status**: Implemented on March 9, 2026
**Companions**:
- `RBAC_AUTHZ_REMEDIATION_IMPLEMENTATION.md`
- `RBAC_AUTHZ_REMEDIATION_TASKS.md`

---

## Summary

This document defines the target authorization model for GHOST.

The current system has real RBAC structure, but authorization truth is still split across:

- JWT claim strings
- route grouping in `crates/ghost-gateway/src/route_sets.rs`
- middleware in `crates/ghost-gateway/src/api/rbac.rs`
- handler-local checks in files like `crates/ghost-gateway/src/api/safety.rs`
- manually maintained API documentation and comments

That split is now the main risk.

The goal of this remediation is to replace stringly-typed, route-local, and handler-local authorization behavior with one typed, auditable, fail-closed authorization model.

This is not a cosmetic cleanup. It is a control-plane hardening effort.

---

## Why Change

The current system has five structural authz problems.

### 1. Policy truth is fragmented

Authorization requirements are currently expressed in multiple places:

- role parsing
- route assembly
- handler-local conditionals
- comments and OpenAPI descriptions

That means the system can drift while still compiling.

### 2. Role strings are still treated as a public internal contract

The repo still relies on raw role strings like:

- `viewer`
- `operator`
- `admin`
- `superadmin`
- `dev`

This makes privilege behavior easy to patch locally and hard to prove globally.

### 3. `security_reviewer` already exists conceptually but not architecturally

`crates/ghost-gateway/src/api/safety.rs` references `security_reviewer`, but the shared parser in `crates/ghost-gateway/src/api/rbac.rs` does not recognize it.

This is the clearest signal that the current model is too small for the real product.

### 4. Router authorization and business-state validation are mixed together

Examples:

- route middleware answers who may reach a handler
- handler code also answers who may perform a state transition
- forensic review and second confirmation are mixed into the same flow as role checks

Those are different concerns and must be modeled separately.

### 5. Unknown authorization state does not fail hard enough

Today, unrecognized roles are logged and effectively downgraded. That is safer than fail-open, but it is still an underspecified recovery path for a critical control surface.

The target system should treat malformed or unknown privilege state as explicit auth failure unless a deliberate compatibility path says otherwise.

---

## Goals

- define one typed authorization model for the gateway
- keep a simple role hierarchy where hierarchy is correct
- model non-hierarchical privilege as capabilities, not fake ranks
- remove all handler-local raw string role checks
- bind routes to explicit action identifiers rather than informal route groups
- make unknown roles and capabilities fail closed
- make OpenAPI, route protection, and runtime behavior derive from the same policy source
- add an authorization test matrix that proves both allow and deny behavior

---

## Non-Goals

- redesigning all authentication UX in the same pass
- inventing ABAC or a full enterprise IAM system
- changing token transport format beyond what is required for typed authz
- solving tenant isolation, per-resource ownership, or delegated sharing in this document
- replacing state-transition validation with authorization checks

---

## Engineering Bar

This remediation is only acceptable if the final system satisfies all of:

- one typed principal model used by all gateway authorization paths
- one authoritative action registry for privileged operations
- one authorizer used by both middleware and handlers
- no raw string comparisons for role/capability decisions in production code
- unknown privilege state fails closed
- route protection and handler authorization cannot silently diverge
- comments and OpenAPI privilege descriptions are generated from the same source or verified against it
- every privileged action has negative tests, not just happy-path tests

This work is also only acceptable if migration is disciplined:

- no breaking token change without a compatibility story
- no permission broadening hidden inside refactors
- no temporary bypasses in critical paths
- no capability introduced without an explicit owner and scope

---

## Current State

### Current claims model

`crates/ghost-gateway/src/api/auth.rs` defines:

- `sub`
- `role`
- `exp`
- `iat`
- `jti`

That is enough for a pure role hierarchy, but not enough for orthogonal privileges like safety review.

### Current route model

`crates/ghost-gateway/src/route_sets.rs` assembles routes into four broad groups:

- public
- read
- operator
- admin
- superadmin

This is useful, but coarse.

It expresses the minimum route tier, not the full authorization semantics of a specific operation.

### Current middleware model

`crates/ghost-gateway/src/api/rbac.rs` implements a rank-only role parser and a minimum-role check.

This is the correct nucleus, but it is not the full model the product now needs.

### Current handler model

Several handlers still own authorization logic beyond middleware:

- safety routes
- provider key operations
- backup and restore operations
- live execution visibility

Some of those checks are legitimate business-state checks. Some are leaked authorization rules.

The system needs a hard line between the two.

---

## Target Authorization Model

## Decision 1: separate authentication output from authorization input

JWT decoding should not be the final privilege model.

Authentication produces raw claims.
Authorization consumes a typed principal.

### Target types

```rust
pub struct Principal {
    pub subject: String,
    pub base_role: BaseRole,
    pub capabilities: BTreeSet<Capability>,
    pub auth_mode: AuthMode,
    pub token_id: Option<String>,
    pub authz_version: u16,
    pub issuer: Option<String>,
}

pub enum BaseRole {
    Viewer,
    Operator,
    Admin,
    SuperAdmin,
}

pub enum Capability {
    SafetyReview,
}

pub enum AuthMode {
    Jwt,
    LegacyToken,
    NoAuthDev,
}

pub struct AuthorizationContext<'a> {
    pub action: Action,
    pub route_id: RouteId,
    pub transport: TransportKind,
    pub resource: ResourceContext<'a>,
}

pub enum TransportKind {
    Http,
    WebSocket,
    Cli,
    Internal,
}

pub enum ResourceContext<'a> {
    None,
    Agent { agent_id: Option<uuid::Uuid> },
    Session {
        session_id: &'a str,
        owner_subject: Option<&'a str>,
    },
    LiveExecution {
        execution_id: &'a str,
        owner_subject: Option<&'a str>,
    },
    ProviderKey { env_name: Option<&'a str> },
    BackupArchive,
}

pub enum RouteId {
    SafetyStatus,
    SafetyPauseAgent,
    SafetyQuarantineAgent,
    SafetyResumeAgent,
    SafetyKillAll,
    AdminBackupCreate,
    AdminBackupList,
    AdminExportData,
    AdminRestoreVerify,
    ProviderKeyRead,
    ProviderKeyWrite,
    ProviderKeyDelete,
}
```

### Why

This gives the system one normalized privilege input regardless of:

- JWT mode
- legacy token mode
- no-auth dev mode

It also prevents the next class of authz failure:

- route allowed, handler denied
- handler allowed, but for the wrong resource class
- future owner-scoped or subject-sensitive rules getting shoved back into ad hoc handler logic

Without `AuthorizationContext`, the design only works for pure role ladders. This system is already more complex than that.

---

## Decision 2: privilege claims must be versioned and typed

The design must define an explicit privilege wire contract rather than vaguely “extending claims later.”

### Target wire shape

```rust
pub struct AuthzClaimsV1 {
    pub sub: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub authz_v: u16,
    pub exp: u64,
    pub iat: u64,
    pub jti: String,
    pub iss: Option<String>,
}
```

### Rules

- `authz_v = 1` is the first typed authz schema.
- Tokens without `authz_v` are legacy role-only claims and are accepted only during a defined migration window.
- Capability names are parsed against a closed enum.
- Unknown capability names are authorization failures, not warnings.
- Capability-bearing tokens may only be minted by explicitly trusted issuer paths.
- Local login behavior must be explicitly documented: what role it mints, whether it can mint capabilities, and under what mode.

### Why

The original draft was too hand-wavy here. Security boundaries do not get to have “we will figure out the claim format during implementation” as a design stance.

---

## Decision 3: keep base roles hierarchical

The base role ordering remains:

- `Viewer < Operator < Admin < SuperAdmin`

This part of the current model is correct and should stay simple.

### Rule

If an action is truly hierarchical, its policy must be expressible as a minimum base role.

Examples:

- read session data
- run workflows
- set provider keys
- create backups
- verify restore
- kill all agents

---

## Decision 4: model `security_reviewer` as a capability, not a role

`security_reviewer` is not a clean level in the hierarchy.

It is a narrow authority over specific safety-review operations.

### Correct model

`security_reviewer` becomes:

- `BaseRole::Operator`
- plus `Capability::SafetyReview`

This remediation deliberately chooses the floor instead of punting it.

If the product later needs a lower-privilege human reviewer account, that should be introduced as a separate, versioned model change. It should not be left ambiguous in this remediation.

### Why

If `security_reviewer` becomes a rank, the system will gradually overgrant it to unrelated surfaces:

- provider keys
- backup/export
- webhook management
- restore

That would be a category error.

---

## Decision 5: every privileged operation gets an `Action`

The central policy unit is not a route group. It is an action.

Examples:

- `SafetyStatusRead`
- `SafetyPauseAgent`
- `SafetyQuarantineAgent`
- `SafetyResumeQuarantine`
- `AdminBackupCreate`
- `AdminBackupList`
- `AdminRestoreVerify`
- `ProviderKeyRead`
- `ProviderKeyWrite`
- `ProviderKeyDelete`

### Rule

Every externally callable privileged operation must have exactly one action identifier.

No production authorization check should talk directly in terms of route paths or raw role strings.

The action inventory is a mandatory artifact, not a suggestion.

---

## Decision 6: policy must support explicit predicates and deny precedence

The original draft’s algebra of:

- minimum role
- all capabilities
- any capabilities

is too weak.

It does not cleanly express:

- `admin OR operator+safety_review`
- auth-mode restrictions
- subject/resource-sensitive rules
- explicit deny precedence

### Target shape

```rust
pub enum PolicyPredicate {
    True,
    MinRole(BaseRole),
    HasCapability(Capability),
    AuthModeIs(AuthMode),
    AuthModeIn(&'static [AuthMode]),
    SubjectMatchesResourceOwner,
    Any(&'static [PolicyPredicate]),
    All(&'static [PolicyPredicate]),
    Not(&'static PolicyPredicate),
}

pub struct PolicyRule {
    pub allow_if: PolicyPredicate,
    pub deny_if: &'static [PolicyPredicate],
    pub audit_on_deny: bool,
}
```

Each action has one policy definition.

### Evaluation rule

- deny predicates are evaluated first
- if any deny predicate matches, the decision is deny
- otherwise `allow_if` must evaluate true
- no implicit allow exists
- policy evaluation is pure and deterministic for a given principal and context

### Example

`SafetyResumeQuarantine` can now be represented cleanly as:

- allow if `Any([MinRole(Admin), All([MinRole(Operator), HasCapability(SafetyReview)])])`
- business-state checks still handle forensic-review and second-confirmation facts

---

## Decision 7: business-state preconditions are not authz

Authorization answers:

- may this principal attempt this action?

Business-state validation answers:

- is this state transition valid right now?

Examples of business-state validation:

- `forensic_reviewed == true`
- `second_confirmation == true`
- expected safety level matches current level
- archive path exists
- restore target is fresh

These checks stay in handler or domain logic, but must execute after authorization.

---

## Decision 8: route binding must derive from action policy and route metadata is mandatory

`crates/ghost-gateway/src/route_sets.rs` should stop being the primary place where privilege semantics are invented.

Routes should be bound through helpers that declare:

- HTTP path
- method
- action
- compatibility requirements

The route layer may still use grouped middleware for efficiency, but the authoritative permission must come from the action registry.

For non-public routes, registration without action metadata is forbidden.

### Target shape

```rust
pub struct RouteAuthorizationSpec {
    pub route_id: RouteId,
    pub action: Action,
    pub compatibility_required: bool,
}
```

This removes the last excuse for route/authz/doc drift.

---

## Decision 9: unknown privilege state fails closed

The target system must reject:

- unknown base roles
- malformed capability values
- impossible role/capability combinations when explicitly disallowed

Downgrading unknown claims to `Viewer` is not sufficient as a long-term model because it hides authorization corruption inside a partial fallback.

### Compatibility exception

If legacy compatibility is required during migration, it must be explicit, time-bounded, and tested.

---

## Decision 10: migration must be mode-gated and shadowed before enforcement

This remediation must not cut directly from legacy behavior to new behavior.

### Target enforcement modes

```rust
pub enum AuthzEnforcementMode {
    LegacyOnly,
    Shadow,
    Enforced,
}
```

### Rules

- `LegacyOnly`: current behavior enforced, new authorizer disabled
- `Shadow`: current behavior enforced, new authorizer evaluated in parallel, mismatches logged and aggregated
- `Enforced`: new authorizer enforced, optional legacy evaluator retained briefly for comparison only

### Required shadow outputs

- action
- route id
- principal summary
- legacy decision
- new decision
- mismatch reason class

### Cutover gate

The system does not move from `Shadow` to `Enforced` until:

- all authz tests pass
- expected compatibility mismatches are exhausted and documented
- no unexplained shadow mismatch remains

### Rollback rule

The previous enforcement mode must remain available until the deletion phase completes.

---

## Decision 11: no-auth mode remains deliberately limited

`dev` remains a compatibility principal for local development only.

It must normalize to:

- `AuthMode::NoAuthDev`
- `BaseRole::Operator`
- zero privileged capabilities

That preserves the current safety posture:

- local work remains usable
- safety/admin/superadmin surfaces still require real auth

---

## Decision 12: authorization decisions must be observable and diffable

Privileged denies are security-relevant events.

The system should log and, where appropriate, audit:

- action attempted
- principal subject
- resolved base role
- resolved capabilities
- denial reason class

The goal is traceability without leaking secrets.

During migration, shadow-mode mismatches are also first-class observability events, not debug noise.

---

## Architectural Invariants

These are hard rules.

### Invariant 1: one principal model

No production authz path may bypass principal resolution.

### Invariant 2: one action registry

No privileged action may exist without a single policy owner.

### Invariant 3: one authorizer

Middleware and handlers must share the same allow/deny semantics.

### Invariant 4: no handler-local role strings

Business handlers may inspect typed authorization state, but may not compare role strings.

### Invariant 5: capability narrowness

Capabilities must be narrow and purpose-specific. They are not a second hidden role ladder.

### Invariant 6: fail closed

Malformed or unknown privilege data must deny access.

### Invariant 7: state preconditions are explicit

Authorization and transition validity must not be conflated.

### Invariant 8: route registration carries policy identity

No non-public route may be mounted without a `RouteAuthorizationSpec`.

### Invariant 9: cutover is reversible until cleanup

The new authorizer may not become a one-way migration until shadow-mode validation is complete and the rollback path is no longer needed.

---

## Migration Strategy

This should be executed in phases.

### Phase 1: freeze inventory and wire contract

- enumerate all privileged actions and routes
- define `AuthzClaimsV1`
- define the exact `security_reviewer` model
- produce the initial action inventory artifact

### Phase 2: typed core and dual evaluator

- introduce `Principal`, `AuthorizationContext`, `Action`, `PolicyPredicate`, `PolicyRule`
- implement legacy evaluator wrapper
- implement new authorizer in parallel
- keep live enforcement on legacy behavior

### Phase 3: shadow mode

- enable `AuthzEnforcementMode::Shadow`
- emit mismatch telemetry and logs
- classify every mismatch as:
  - expected compatibility gap
  - implementation defect
  - documentation defect

### Phase 4: route declaration binding

- require route metadata for non-public routes
- bind routes to `Action`
- remove implicit policy ownership from route grouping

### Phase 5: capability rollout and handler migration

- introduce `Capability::SafetyReview`
- migrate safety and other handler seams to typed authz
- preserve business-state validation as separate logic

### Phase 6: enforced cutover

- switch to `AuthzEnforcementMode::Enforced`
- keep legacy evaluator available only for diff logging
- prove rollback still works

### Phase 7: delete legacy privilege paths

- remove compatibility branches
- reject old unsupported privilege encodings
- remove legacy evaluator after the cutover window closes

---

## Verification Standard

The final authz system must be proven by matrix tests.

Each action must have explicit tests for:

- anonymous
- malformed claims
- legacy role-only claims
- `viewer`
- `operator`
- `admin`
- `superadmin`
- `dev`
- capability-bearing principals where relevant

The test matrix must prove:

- allow behavior
- deny behavior
- no privilege widening under migration
- no unknown-claim fallback widening
- no route/handler mismatch

Additional adversarial tests must cover:

- stale JWT privilege format
- unknown capability names
- mixed capability and role combinations
- shadow-mode mismatch classification
- enforcement-mode rollback
- replay of privileged actions across restarts

Required verification artifacts:

- action inventory
- claims schema decision
- shadow mismatch report
- route-to-action matrix
- compatibility deprecation checklist

---

## Exit Criteria

This remediation is done only when all of the following are true:

- `security_reviewer` is modeled explicitly and consistently
- no production handler compares role strings directly
- route protection and handler authorization derive from one policy source
- the policy source covers all privileged gateway actions
- OpenAPI/comments no longer overstate or understate authz behavior
- the authorization matrix proves correct deny behavior across all action classes
- the action inventory, claims schema decision, route-to-action matrix, and shadow mismatch report exist and are complete
- enforced-mode cutover has shadow evidence behind it
- rollback remains available until legacy deletion is complete

Until then, the system should be treated as partially hardened, not complete.
