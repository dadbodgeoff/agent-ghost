# Proposal Remediation Charter

Date: March 11, 2026

Status: Active

Purpose: define the remediation program for the ADE proposal subsystem and establish the engineering standard the implementation must meet.

## Program Objective

Rebuild the proposal subsystem into a single coherent product surface and contract model so that:

- proposal creation, review, supersession, timeout, approval, rejection, and history behave identically across storage, API, SDK, WebSocket, service worker, and ADE UI
- the operator has one canonical review experience
- live state is truthful under race, retry, stale review, offline replay, and multi-tab conditions
- tests and contract gates prevent the current class of drift from recurring

## Non-Negotiable Standard

This work is held to the following bar:

- No proposal lifecycle concept may have more than one semantic authority.
- No UI may infer lifecycle state from legacy field shapes when a canonical state machine exists.
- No public contract may expose both legacy and canonical interpretations without an explicit migration rule.
- No operator workflow may depend on manual refresh to discover newly created work.
- No mutation flow may claim correctness while relying on optimistic UI state that can disagree with server truth.
- No retry, replay, or offline mutation path may bypass idempotency, stale-review, or supersession safeguards.
- No tests may mock impossible proposal states as the default happy path.
- No duplicated proposal review surfaces may remain in production without a documented primary and a documented reason for the secondary.

## Scope

This remediation covers:

- proposal lifecycle persistence semantics
- proposal list/detail/decision REST contracts
- proposal WebSocket event contracts
- SDK wrappers and generated-type alignment for proposal routes
- ADE operator proposal surfaces
- navigation, notifications, and command entry points into proposal review
- service worker mutation replay behavior for proposal decisions
- proposal-domain tests, architecture gates, and release criteria

This remediation does not redesign the broader agent governance system outside the proposal domain except where those systems directly participate in proposal state or operator review behavior.

## Out of Scope

These are not primary goals unless they block proposal correctness:

- redesigning non-proposal ADE pages
- changing the underlying proposal validation dimensions
- reworking unrelated dashboard styling or layout systems
- broad migration of all legacy gateway APIs outside the proposal namespace

## Canonical User Outcomes

The operator must be able to:

1. See every pending proposal that requires human review without ambiguity.
2. Receive live notification when a new reviewable proposal is created.
3. Open one canonical detail view with authoritative state, history, and decision affordances.
4. Approve or reject exactly once with explicit stale/superseded handling.
5. Trust that what ADE shows matches backend truth even after retries, reconnects, or offline replay.

## Required Program Deliverables

The implementation must produce:

1. A canonical proposal domain model in code and docs.
2. A single canonical operator surface in ADE.
3. REST and WebSocket contracts aligned to that model.
4. Updated SDK wrappers and generated types.
5. Contract and regression tests that exercise real state semantics.
6. A remediation closeout document after implementation is complete.

## Success Definition

This program is complete only when:

- every human-review proposal appears in the canonical pending queue
- auto-resolved proposals never appear as human-pending
- proposal creation and decision both update live ADE state
- stale review conflicts are deliberate, explicit, and operator-readable
- offline replay cannot create invalid duplicate or stale decisions
- one proposal review route is canonical in nav, notifications, and commands
- verification gates cover the full proposal path end to end

## Ownership Model

- Backend authority: `crates/ghost-gateway/src/api/goals.rs`
- Persistence authority: `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- Lifecycle production authority: `crates/ghost-agent-loop/src/runner.rs`
- SDK authority: `packages/sdk/src/goals.ts` and generated types
- ADE operator authority: one canonical route to be selected during remediation
- WebSocket authority: `crates/ghost-gateway/src/api/websocket.rs`
- Release gate authority: proposal-specific tests and parity checks defined in this packet
