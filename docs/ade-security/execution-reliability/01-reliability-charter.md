# Reliability Charter

## Objective

Build a live-execution subsystem for ADE routes that is safe under:

- client disconnect
- server restart
- worker crash
- DB write failure
- lease takeover
- duplicate request replay
- concurrent cancel / complete races

The system must not silently lose durable state and must not silently duplicate side effects inside the supported execution envelope.

## Reliability Bar

The target is "0.0001 engineer" quality. In practice that means:

- no known last-write-wins race on execution state
- no known path where recovery claims success after durable-write loss
- no known path where a supported side effect can be replayed twice without an explicit operator-visible recovery state
- deterministic recovery behavior for every execution status
- a complete failure matrix backed by tests

## Supported Reliability Semantics

The implementation must distinguish three execution classes:

1. Replay-safe
- read-only operations
- deterministic recomputation with no side effects

2. Journaled side-effecting
- operations whose start/commit/result can be durably checkpointed
- recovery can resume or replay from committed step records

3. Unsupported for exact-once
- arbitrary external side effects where the platform cannot prove idempotency or commit state
- these must fail closed on reliable routes, or require explicit operator workflow outside the exact-once guarantee

## Product Guarantees

For reliable ADE routes:

- accepted work has a durable execution record
- each execution has a single authoritative state machine
- each durable state transition is compare-and-swap protected
- each streamed event is persisted before client delivery
- terminal success is not published until the canonical result record is durable
- cancellation is registered immediately at acceptance
- recovery either:
  - resumes from durable checkpoints,
  - replays committed output,
  - or enters an explicit `recovery_required` / `needs_review` state

## Explicit Non-Guarantees

The system must state these plainly:

- arbitrary shell commands are not exactly-once unless wrapped in a supported journaled execution adapter
- external skills with untracked side effects remain blocked on reliable routes until they satisfy the journal contract
- "best effort plus warning" is not acceptable for the reliable execution surface

## Exit Criteria

This program is not done until:

- the failure matrix in `05-verification-and-runbooks.md` passes
- the blocking and streaming ADE routes share the same execution semantics
- unsupported side-effect classes are enforced by policy, not comments
- operator-facing recovery and review states are observable through API and UI
