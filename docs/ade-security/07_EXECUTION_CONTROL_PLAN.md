# ADE Security Remediation: Execution Control Plan

Status: draft for implementation orchestration on March 11, 2026.

This document defines how the remediation is executed, governed, and verified.
It exists to prevent scope drift, local optimization, and incomplete closeout.

## 1. Program Objective

Deliver the ADE Security remediation from audited findings to release-ready
implementation with:

- contract parity
- permission parity
- live-state parity
- export/evidence parity
- automated proof

## 2. Execution Model

The work is executed as a controlled sequence of work packages.

Each work package must move through these states:

1. `Ready`
2. `In Progress`
3. `Code Complete`
4. `Verified`
5. `Accepted`

No work package may skip `Verified`.

## 3. Governing Rules

- No code change without a mapped work package in `09_EXECUTION_TRACKER.md`.
- No contract change without updating `03_CONTRACT_MATRIX.md`.
- No privileged UX change without validating shell and page parity.
- No UI evidence change without validating export parity.
- No subsection may silently degrade to empty state.
- No package is accepted if a downstream dependency is knowingly broken.

## 4. Decision Hierarchy

If multiple docs seem to conflict:

1. `03_CONTRACT_MATRIX.md`
2. `05_VERIFICATION_AND_RELEASE_GATES.md`
3. `08_WORKSTREAM_PLAN.md`
4. `09_EXECUTION_TRACKER.md`
5. `06_AGENT_HANDOFF.md`

## 5. Mandatory Execution Rhythm

Each work package follows this loop:

1. run pre-change drift check
2. implement only the scoped package
3. run local verification for the touched area
4. run post-change drift check
5. record evidence and status

If post-change drift fails, the package is not `Code Complete`.

## 6. Evidence Requirements Per Package

Each completed work package must leave:

- code changes
- updated docs if contracts or scope changed
- verification notes
- explicit pass/fail result
- residual risk note, if any

## 7. Blocker Policy

The execution owner may stop only for:

- missing required backend capability that cannot be inferred safely
- contradictory existing contract owner docs
- unrelated repo changes that directly conflict with the current package
- failing verification that indicates a wider architectural issue

All other ambiguity is resolved by choosing the stricter fail-closed path.

## 8. Cross-Surface Control Points

These checkpoints are mandatory because this remediation spans multiple layers.

### Control Point A: Gateway/SDK parity

Must pass before major dashboard rewiring.

### Control Point B: Shell/UI permission parity

Must pass before signoff on any action gating.

### Control Point C: Query/export parity

Must pass before signoff on any filter work.

### Control Point D: Real-time/state parity

Must pass before final route acceptance.

## 9. Completion Criteria

Execution is complete only when:

- every work package in `09_EXECUTION_TRACKER.md` is `Accepted`
- every trace item in `10_TRACEABILITY_MATRIX.md` has evidence
- all release gates in `05_VERIFICATION_AND_RELEASE_GATES.md` pass
- `12_SIGNOFF_PACKET.md` can be filled with real outputs, not intent
