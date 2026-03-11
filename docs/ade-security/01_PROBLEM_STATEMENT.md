# ADE Security Remediation: Problem Statement

Status: draft for implementation handoff on March 11, 2026.

## Mission

Remediate the ADE Security surface so it behaves as a single coherent system
across:

- gateway REST APIs
- websocket event flows
- SDK types
- dashboard route surfaces
- shell-level shortcuts and command affordances

The resulting system must fail closed, surface degraded state honestly, and
never advertise actions the current principal cannot perform.

## Audited Problems

The current implementation has six material classes of failure.

### 1. Safety state is not rendered truthfully

The dashboard parses `platform_level` as a number while the gateway returns
enum names such as `Normal`, `Pause`, `Quarantine`, and `KillAll`. This can
render a non-normal platform state as `L0`.

Impact:

- the top-line kill-state indicator cannot be trusted
- the most safety-critical state can be visually downgraded to normal

### 2. Security filters are not contract-correct

The Security page offers hard-coded event types and severity filters that do not
match the gateway audit semantics.

Current issues:

- multi-select severities are joined into a comma-separated string
- backend filtering is exact-match, not set-based
- UI vocabulary and backend vocabulary diverge
- offered event types omit actual security events written by the gateway

Impact:

- false empty states
- partial evidence
- broken operator trust in the audit surface

### 3. Failure is masked as empty state

Sandbox review fetch failures are converted to empty results. This makes
authorization failures, endpoint regressions, or degraded backend behavior look
identical to “there are no reviews.”

Impact:

- operational blindness
- misleading incident response behavior
- inability to distinguish clean state from failed state

### 4. The page is not live as a cohesive surface

Security actions write audit entries, but the Security page only refreshes some
subsections. The audit view becomes stale after real changes.

Impact:

- the page is not a live security console
- operators cannot trust the page after the first event

### 5. Permission awareness is inconsistent across the ADE

The gateway correctly enforces privilege on destructive routes, but the shell
and the Security page still advertise actions broadly.

Examples:

- `kill-all` is surfaced to non-superadmins
- sandbox review decisions are surfaced to operators who lack
  `safety_review`

Impact:

- user-visible false affordances
- inconsistent security posture between shell and backend
- unnecessary runtime failures for expected-denied actions

### 6. Contracts have drifted between gateway and SDK

The SDK and page model fields that the gateway no longer returns, while the
gateway returns fields the frontend does not model or display.

Impact:

- missing security telemetry on the page
- weakened type safety
- hidden integration drift

## Desired End State

The ADE Security system must satisfy all of the following.

- Safety state is represented canonically and rendered accurately.
- Audit filters and exports reflect the same canonical filter model.
- Every section distinguishes `empty`, `loading`, `partial`, and `error`.
- Security events refresh all dependent views or stores.
- UI affordances are role- and capability-aware before the user acts.
- SDK and gateway contracts are aligned and explicitly typed.
- Tests prove behavior across normal, denied, and degraded states.

## Non-Goals

The following are out of scope unless required to satisfy the goals above.

- redesigning unrelated dashboard pages
- changing backend authorization semantics beyond UI/contract alignment
- introducing new security business workflows not already supported by the
  gateway
- broad visual redesign unrelated to correctness, trust, or operability

## Engineering Rules

These are hard constraints for implementation.

- No hidden fallback from failure to empty state.
- No privileged action may be rendered without a matching permission check.
- No hard-coded filter vocabulary that duplicates backend authority.
- No stringly typed safety-level parsing in the UI if the backend can return a
  typed value.
- No partial fix that corrects the page but leaves the shell or shortcut layer
  inconsistent.
- No release without automated coverage for the remediated paths.

## Primary Risks

- fixing only the `/security` page and leaving global shell affordances
  inconsistent
- fixing UI labels without fixing wire contracts
- adding new local enums that drift from gateway behavior again
- shipping without degraded-path tests

## Definition Of Success

This remediation is successful only when:

- the Security page is trustworthy as an operational console
- permission-aware behavior is consistent across shell and page surfaces
- the SDK contract and gateway payloads match
- filters, exports, and real-time updates produce the same evidence set
- the release gates in `05_VERIFICATION_AND_RELEASE_GATES.md` all pass
