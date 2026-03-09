# Live Hardening Phase 2 Plan

## Baseline

As of March 9, 2026, the live POC coverage target is complete.

Current proof points:

- latest `repo-live` passed: [artifacts/live-repo-suites/20260309-211029/summary.json](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/artifacts/live-repo-suites/20260309-211029/summary.json)
- latest `critical-live` passed: [artifacts/live-critical-suites/20260309-211031/summary.json](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/artifacts/live-critical-suites/20260309-211031/summary.json)
- uncovered counts are zero in [artifacts/live-reporting/live-report.json](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/artifacts/live-reporting/live-report.json)

Current hardening commands:

- `pnpm audit:soak-live`
- `pnpm audit:prune-live`

Operator guide:

- [LIVE_OPERATOR_RUNBOOK.md](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/LIVE_OPERATOR_RUNBOOK.md)

This phase is not about adding missing sections. It is about making the existing live verifier reliable enough for unattended operation, regression gating, and operator trust.

## Goal

Turn the full-coverage live verifier into a production-grade operating system for validation.

Success means:

- repeated runs are stable and flake-classified
- failures are attributable and easy to rerun
- suites can gate releases and run nightly without manual babysitting
- artifact growth is controlled
- performance regressions become visible
- external-provider gaps are explicitly certified as mocked or real

## Workstreams

### 1. Soak and Flake Validation

Purpose:

- prove the suite is stable across repetition, not just single green runs

Commands:

- `pnpm audit:critical-live`
- `pnpm audit:repo-live`

Implementation:

- add a repeat runner for `critical-live` and `repo-live`
- classify failures into:
  - product regression
  - harness defect
  - environment failure
  - transient external/provider failure
- capture per-run duration and failure reason in aggregate output

Acceptance criteria:

- `critical-live` can pass `10` consecutive runs on temp state
- `repo-live` can pass `3` consecutive runs on temp state
- every non-pass result is tagged with a failure class
- rerun output names the exact failing component and artifact directory

### 2. CI and Nightly Wiring

Purpose:

- make the verifier run automatically instead of only by hand

Commands:

- `pnpm audit:preflight-live`
- `pnpm audit:critical-live`
- `pnpm audit:repo-live`

Implementation:

- add CI entry points for:
  - preflight on every workflow that uses the live harness
  - `critical-live` on protected branches or release candidates
  - `repo-live` on nightly schedule
- ensure exit codes are stable and summaries are persisted as artifacts

Acceptance criteria:

- one CI path runs `preflight-live` plus `critical-live`
- one scheduled path runs `repo-live`
- failed CI jobs expose the summary JSON and artifact directory references
- rerunning a failed suite does not require editing local config

### 3. Fault Injection and Recovery Journeys

Purpose:

- prove the system survives the kinds of live interruptions that happen in production

Commands:

- `pnpm audit:poc-live -- --journeys studio-ws-reconnect,studio-reload-recover`
- `pnpm audit:repo-live`

Implementation:

- add controlled fault modes for:
  - gateway restart during a browser-driven flow
  - websocket disconnect and reconnect during active session work
  - slow provider response
  - partial child-suite failure with resume or rerun support
- preserve fault cause in run summary

Acceptance criteria:

- at least one gateway-restart journey exists and passes
- at least one slow-path journey exists and passes within defined timeout budget
- repo aggregate output can show partial component failure without losing child artifacts
- reconnect/recovery failures are distinguishable from ordinary app errors

### 4. External Integration Certification

Purpose:

- make it explicit which integrations are proven with mocks and which are proven against real third-party systems

Commands:

- `pnpm audit:io-live`
- `pnpm audit:distributed-live`
- `pnpm audit:repo-live`

Implementation:

- mark each external dependency as one of:
  - `mocked`
  - `local real`
  - `third-party real`
  - `not certified`
- extend reporting so mocked-only coverage is visible in the live report
- add opt-in real-provider mode where safe credentials exist

Acceptance criteria:

- live report includes integration certification status
- OAuth, webhooks, channels, A2A peers, and marketplace paths are each labeled
- mocked coverage and real-provider coverage are not conflated
- repo operator can tell what still depends on local simulation

### 5. Performance Budgets and Trend Tracking

Purpose:

- detect suites that still functionally pass but are degrading

Commands:

- `pnpm audit:critical-live`
- `pnpm audit:repo-live`
- `pnpm audit:index-live`

Implementation:

- record per-suite and per-component duration history
- add basic threshold checks for:
  - preflight duration
  - critical suite duration
  - repo suite duration
  - selected hot journeys like Studio and runtime chat
- emit warnings or failures when thresholds are crossed

Acceptance criteria:

- summary JSON includes duration for each child component
- artifact index includes historical duration data
- at least one configurable performance threshold exists for `critical-live`
- regressions are visible without opening raw logs

### 6. Artifact Lifecycle and Operator UX

Purpose:

- keep the verifier usable over long unattended runs

Commands:

- `pnpm audit:index-live`
- `pnpm audit:repo-live`

Implementation:

- add pruning helpers for old successful runs
- keep newest failure artifacts by default
- add one operator-facing summary page or markdown doc that explains:
  - what to run
  - how to inspect failures
  - how to rerun one component
  - how cleanup works

Acceptance criteria:

- old successful artifact directories can be pruned without touching the latest failure set
- the artifact index remains valid after pruning
- one operator doc exists for running and debugging the live suite
- storage cleanup rules are executable, not just described

## Recommended Execution Order

1. soak and flake validation
2. artifact lifecycle and operator UX
3. CI and nightly wiring
4. fault injection and recovery journeys
5. performance budgets and trend tracking
6. external integration certification

This order keeps the highest operational risk first: trust the current suite, keep it maintainable, then expand its production authority.

## Exit Criteria

Phase 2 is complete when all of these are true:

- `repo-live` and `critical-live` have repeat-run evidence, not just one-off green runs
- CI or scheduled automation can run the verifier without local edits
- artifacts are indexed, prunable, and attributable
- recovery and restart behavior are tested intentionally
- mocked versus real integration coverage is visible in reports
- duration history exists and at least one regression budget is enforced

## Immediate Next Step

Start with soak validation and artifact lifecycle, because they will expose whether the current full-coverage suite is operationally trustworthy.
