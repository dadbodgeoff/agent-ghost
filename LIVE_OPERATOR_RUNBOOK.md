# Live Operator Runbook

## Purpose

Use this runbook to operate the live verifier after coverage completion.

The main commands are:

- `pnpm audit:preflight-live`
- `pnpm audit:critical-live`
- `pnpm audit:repo-live`
- `pnpm audit:soak-live`
- `pnpm audit:prune-live`
- `pnpm audit:coverage-live`
- `pnpm audit:index-live`

## Standard Order

For a normal manual validation pass:

1. `pnpm audit:preflight-live -- --prune-old-artifacts --keep-recent-runs 20`
2. `pnpm audit:critical-live -- --keep-artifacts`
3. `pnpm audit:repo-live -- --keep-artifacts`

For a stability check:

1. `pnpm audit:soak-live -- --target critical --runs 10 --keep-artifacts`
2. `pnpm audit:soak-live -- --target repo --runs 3 --keep-artifacts`

## What Each Command Is For

`pnpm audit:preflight-live`
- Validates toolchain, disk, ports, Playwright, and gateway build readiness.

`pnpm audit:critical-live`
- Runs the release-confidence slice across the main live components.

`pnpm audit:repo-live`
- Runs the full top-level orchestrator and refreshes the reporting bundle.

`pnpm audit:soak-live`
- Repeats `critical-live` or `repo-live` to measure stability and classify failures.

`pnpm audit:prune-live`
- Removes old artifact runs while preserving recent successes and failures.

`pnpm audit:coverage-live`
- Rebuilds crate, route-family, and dashboard-page coverage manifests.

`pnpm audit:index-live`
- Rebuilds the artifact index used by the live report.

## How To Read A Failure

Every suite prints:

- `Artifacts: /absolute/path`
- `Report: /absolute/path`

Start with:

1. the suite `summary.json`
2. the child component log referenced in that summary
3. the child artifact directory for the failing component
4. [artifacts/live-reporting/live-report.json](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/artifacts/live-reporting/live-report.json)
5. [artifacts/live-reporting/artifact-index.json](/Users/geoffreyfernald/Documents/New%20project/agent-ghost/artifacts/live-reporting/artifact-index.json)

Failure classification meanings:

- `product_regression`: the app behavior or assertions failed
- `harness_defect`: selectors, waits, or test harness behavior failed
- `environment_failure`: local machine or toolchain preconditions failed
- `transient_external_provider_failure`: network/provider instability or temporary external errors

## How To Rerun One Slice

Examples:

- `pnpm audit:poc-live -- --journeys studio-tool --keep-artifacts`
- `pnpm audit:runtime-live -- --keep-artifacts`
- `pnpm audit:io-live -- --keep-artifacts`
- `pnpm audit:surface-live -- --keep-artifacts`
- `pnpm audit:critical-live -- --components poc,runtime,io --keep-artifacts`
- `pnpm audit:repo-live -- --suites preflight,critical --keep-artifacts`

## Artifact Retention

For controlled cleanup:

- `pnpm audit:prune-live -- --keep-success-runs 5 --keep-failure-runs 10`
- `pnpm audit:prune-live -- --dry-run --keep-success-runs 5 --keep-failure-runs 10 --keep-artifacts`

Rules:

- keep recent failed runs longer than successful runs
- use `--dry-run` first if you are unsure
- run `pnpm audit:index-live` or `pnpm audit:repo-live` after manual artifact deletion

## Notes

- All suites are designed to run on temp state; do not point them at persistent user data.
- If disk pressure appears, prune artifacts before using `cargo clean`.
- `critical-live` is the narrow release gate.
- `repo-live` is the broad full-system validation command.
