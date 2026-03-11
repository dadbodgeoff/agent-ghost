# Integrity Hardening Program

## Status

- Author: Codex
- Date: 2026-03-07
- Scope: desktop trust boundary, mutation integrity, proposal lineage, replay semantics, rollout compatibility
- Audience: maintainers, staff engineers, release owners

## Executive Summary

The repo has a good architectural base: a gateway-centered control plane, centralized routing and RBAC, typed storage access, and some replay and migration discipline already in place. The remaining risk is not that the system lacks structure. The remaining risk is that approved paths can still behave unsafely under renderer compromise, retry, stale human decisions, restart, and mixed-version rollout.

There are two top-tier problems:

1. the desktop renderer can cross into host capabilities too easily
2. the decision path for mutable domain state is not yet replay-safe, review-bound, or canonically versioned

This program fixes those problems in a narrow order:

1. collapse renderer host access
2. make logical writes replay-safe and attributable
3. make approvals and supersession lineage canonical in storage
4. either finish replay safety or remove fake safety rails
5. add mixed-version compatibility checks

The intent is not to rewrite Agent Ghost. The intent is to close the few gaps that can still produce silent corruption or privilege collapse.

Execution tasks live in:

- [INTEGRITY_BUILD_TASKS.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_BUILD_TASKS.md)

Testing doctrine lives in:

- [INTEGRITY_TEST_DOCTRINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_TEST_DOCTRINE.md)

## Current Standing

### What is good

- gateway remains the main authority for business routes and RBAC
- the codebase already separates transport, gateway, storage, and desktop concerns
- WebSocket replay and resync have a gap-detecting model instead of blindly accepting out-of-order traffic
- storage migrations already take backups and verify terminal schema version

### What is not good enough

- the renderer still has access to broad host-facing Tauri capabilities
- mutating requests do not yet carry stable logical operation identity
- approvals and rejections are not durably bound to reviewed revision or parent lineage
- supersession exists in process memory more than in canonical storage
- client replay safety is partly signaled but not consistently enforced
- desktop and gateway do not negotiate compatibility before mixed-version operation

## Findings Reframed

### P0: Renderer compromise can collapse into host compromise

Today the most dangerous boundary is the desktop one. If the renderer is compromised through XSS, dependency compromise, or malicious content injection, the available Tauri capabilities can cross the intended UI/runtime boundary and reach local process, file, and secret surfaces.

The fix is not "reduce command count." The fix is capability minimization:

- deny by default per window
- remove renderer access to `fs`, `process`, `shell`, `pty`, and renderer-readable secret storage unless proven necessary
- replace broad sidecar/process launching with fixed-purpose typed commands

### P0: Decision path is not replay-safe, review-bound, or canonically versioned

Three earlier findings are really one integrity problem:

- approvals and rejections are keyed too loosely
- supersession lineage is not fully canonical in storage
- operation identity/idempotency is missing across retries and repair flows

Together, that means the system can struggle to distinguish:

- retry-after-commit
- duplicate execution
- stale approval against an older proposal
- approval of a proposal whose replacement lineage was only remembered in memory

This is the main silent-corruption path in the current architecture.

### P1: Client and gateway disagree about replay protection

The service worker and some client code signal replay safety, but the full invariant is not enforced end to end. That creates false confidence and makes failures harder to reason about.

### P1: Mixed-version rollout has no compatibility contract

Desktop, dashboard, CLI, and gateway can evolve at different speeds. Without a connect-time compatibility check, a downgraded or lagging client can operate on semantics it no longer understands.

### P2: Desktop startup trusts an unverified pidfile

This is a real footgun, but it is secondary compared with the broader renderer/host boundary issue. It should be fixed during desktop hardening, not treated as the main security story.

## Target Architecture

### Workstream 1: Desktop capability boundary

Principle: renderer is hostile.

Target state:

- renderer cannot read or mutate host state except through narrow typed commands
- auth tokens and long-lived secrets are not available through renderer-readable general-purpose storage
- Tauri capabilities are granted per window and per action, not globally
- no generic command proxy or arbitrary sidecar argument surface exists

### Workstream 2: Operation identity and idempotent execution

Principle: one logical write has one durable identity.

Target state:

- every logical mutation carries `operation_id` and `idempotency_key`
- each retry gets a fresh transport-level `request_id`
- gateway owns dedupe and replay
- audit and repair tooling can trace one user action across attempts

Detailed design lives in:

- [REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md)

### Workstream 3: Canonical proposal lineage and decision binding

Principle: a human decision must be bound to the exact thing reviewed.

Target state:

- proposals persist parent revision and supersession lineage in storage
- decision routes require expected state and reviewed revision
- only one terminal outcome is possible for a proposal lineage branch
- stale approvals fail transactionally, not by convention

Minimum storage additions:

- `parent_goal_revision`
- `superseded_by_proposal_id`
- explicit status enum with terminal constraints
- uniqueness and check constraints that enforce one live proposal per lineage slot where appropriate

Detailed design lives in:

- [PROPOSAL_LINEAGE_AND_DECISION_STATE_MACHINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/PROPOSAL_LINEAGE_AND_DECISION_STATE_MACHINE.md)

### Workstream 4: Honest replay and restore semantics

Principle: safety rails must be real or absent.

Target state:

- offline queues persist full operation envelope
- queues are bound to actor plus session epoch
- logout, auth reset, or account switch invalidates queued writes
- resync paths detect gaps and force snapshot or replay, not local guessing

### Workstream 5: Compatibility handshake

Principle: mixed versions fail closed.

Target state:

- gateway advertises supported desktop, dashboard, and CLI version ranges
- clients block unsafe operation on incompatible pairs
- migrations and rollback paths are tested across N-2, N-1, and current

## Implementation Sequence

### Phase 1: Collapse desktop trust

- tighten Tauri capability grants
- remove renderer-readable secret storage for auth material
- replace broad host plugins with typed runtime functions
- fix pidfile validation while touching startup control paths

Release gate:

- renderer compromise can no longer directly reach arbitrary host capabilities

### Phase 2: Add operation identity and idempotent writes

- implement the operation envelope across SDK, CLI, dashboard, and gateway
- add gateway journal and replay semantics
- make highest-risk write routes idempotent first

Release gate:

- duplicate retries return original committed result
- mismatched idempotency reuse fails loudly

### Phase 3: Canonicalize decision lineage

- extend proposal schema
- add transactionally enforced decision preconditions
- reject stale review actions at the gateway and storage layer

Release gate:

- restart, concurrency, and retry cannot cause stale proposal approval

### Phase 4: Finish or remove partial replay safety

- either make service-worker replay and restore real
- or remove partial stale-session signaling until real enforcement exists

Release gate:

- every claimed replay invariant is testable and enforced end to end

### Phase 5: Add compatibility contract

- expose supported client/runtime ranges
- fail closed on incompatible mixes
- test forward, backward, and rollback behavior

Release gate:

- mixed desktop and gateway versions cannot silently operate with incompatible semantics

## Test Program

### Security tests

- renderer XSS attempts all registered Tauri paths
- compromised dependency simulation attempts file, process, and token access
- auth/logout/account-switch tests verify durable local invalidation

### Integrity tests

- retry-after-commit returns replayed response, not duplicate write
- same idempotency key plus different payload returns conflict
- approve vs reject races converge to one valid terminal result
- stale approval against superseded proposal fails

### Recovery tests

- kill the app after commit and before response
- restart during queue replay
- restore after logout and after account switch

### Rollout tests

- N-2, N-1, and current client versus current gateway
- rollback after local migration
- partial desktop rollout during gateway deploy

## What Success Looks Like

The system should be able to make the following statements and have them be true:

- a renderer compromise is no longer equivalent to host compromise
- a human approval is bound to the exact proposal revision reviewed
- one logical write commits once, even if transport attempts happen many times
- replay safety is enforced by the gateway, not implied by the client
- unsupported client and gateway pairs fail closed instead of drifting silently

## Bottom Line

Agent Ghost does not need a new architecture. It needs the current architecture made strict in the places where trust and correctness matter.

The right framing is:

- good foundation
- real P0 trust and integrity gaps
- fixable with a focused hardening program

That is the posture I would take going into implementation and release planning.
