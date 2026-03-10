# CI Overnight Plan

## Objective

Bring the repository to a state where CI is a reliable gate for active deliverables, not just a set of baseline workflow files.

This plan is for an autonomous overnight pass. The agent should continue iterating until:

- all required checks are green, or
- a hard blocker is reached that cannot be resolved safely without user input, or
- the user stops the run.

## Scope

Active surfaces currently in scope:

- Rust workspace at repo root
- Dashboard in `dashboard/`
- SDK in `packages/sdk/`
- Browser extension in `extension/`
- Desktop app in `src-tauri/`
- Existing GitHub Actions workflows in `.github/workflows/`

Out of scope for the first stabilization pass unless needed later:

- release signing/notarization
- benchmark tuning
- long-running live audit suites as required PR gates

## Baseline Findings

Observed from local baseline checks on 2026-03-10:

- `cargo fmt --all -- --check` fails due formatting drift in active Rust changes.
- `cargo test --workspace --no-run` fails with a real compile blocker:
  - `crates/ghost-integration-tests/examples/live_smoke_test.rs`
  - `crates/ghost-gateway/src/autonomy.rs`
  - Failure: `std::sync::RwLockReadGuard<'_, AgentRegistry>` is held across an async boundary, making the future non-`Send`.
- `pnpm install --frozen-lockfile` succeeds.
- `pnpm lint` fails in `dashboard/` with many ESLint errors in `dashboard/scripts/**/*.mjs`.
- `pnpm --dir dashboard check` passes.
- `cargo check --manifest-path src-tauri/Cargo.toml` passes.
- The repo worktree is already dirty, so the overnight pass must not revert unrelated changes.

Additional CI coverage gaps:

- `src-tauri/` is not currently part of the main CI workflow.
- Root `pnpm typecheck` does not cover the dashboard because the dashboard contract is `pnpm --dir dashboard check`.
- SDK coverage exists indirectly via Turbo, but its required contract should be made explicit.
- Extension lint exists in CI, but extension build/typecheck are not first-class required gates.

## Required End State

By the end of the overnight pass, the repository should meet all of the following:

- A clean checkout can run all required PR checks without manual intervention.
- Every active surface has at least one explicit CI gate.
- Required checks are deterministic and appropriate for pull requests.
- Slow or flaky checks are separated from the required PR path.
- CI commands are documented in a short runbook.

## Required PR Gates

Minimum required gates for pull requests:

- Rust format: `cargo fmt --all -- --check`
- Rust lint: `cargo clippy --workspace --all-targets -- -D warnings`
- Rust tests: `cargo test --workspace`
- Architecture guards from root Python scripts
- Workspace install: `pnpm install --frozen-lockfile`
- SDK build/test/typecheck
- Dashboard build
- Dashboard lint
- Dashboard check
- Extension lint
- Extension typecheck
- Extension build
- Desktop smoke build: `cargo check --manifest-path src-tauri/Cargo.toml`

Checks that should likely remain non-blocking or nightly until stable:

- Dashboard Playwright E2E
- benchmark workflow
- long-running live audit suites

## Execution Order

Phase 1: Stabilize the real blockers

- Fix the Rust compile blocker in `ghost-gateway` autonomy bootstrap flow.
- Decide whether formatting drift should be corrected incrementally or with a bounded formatting pass on touched files only.
- Fix dashboard lint failures by correcting ESLint targeting, globals, or script-specific rules rather than suppressing issues blindly.

Phase 2: Normalize the command contract

- Make the canonical CI commands explicit at the root or in workflow steps.
- Avoid hidden coverage gaps where a surface exists but is not part of the required command set.
- Ensure generated artifacts and freshness checks are deterministic in clean runners.

Phase 3: Expand CI coverage

- Update `.github/workflows/ci.yml` so every active surface has an explicit job.
- Add desktop/Tauri smoke validation to CI.
- Add extension build/typecheck if missing from required jobs.
- Keep dashboard `check` separate from generic `typecheck` if the script naming stays different.

Phase 4: Reduce CI brittleness

- Add concurrency cancellation for duplicate PR runs.
- Upload useful artifacts on failure where it helps debugging.
- Split slow jobs from required jobs if they create PR friction.
- Confirm dependency/tool versions are pinned consistently.

Phase 5: Final verification

- Re-run the full required check set from a clean local state as much as feasible.
- Confirm CI workflow YAML matches the actual command contract.
- Write a short runbook describing required checks and their purpose.

## Autonomy Rules

The overnight pass should follow these rules:

- Do not revert or overwrite unrelated user changes.
- Prefer fixing root causes over loosening gates.
- Do not disable a failing check unless it is being intentionally moved to nightly/non-blocking status with a clear reason.
- Do not count a surface as covered unless CI actually executes a meaningful command for it.
- Avoid broad repo-wide formatting churn unless it is necessary and bounded.
- Do not run multiple root Turbo pipelines concurrently against the same workspace while validating locally.

## Blockers That Require User Input

Stop and ask only if one of these occurs:

- A fix requires choosing whether `src-tauri/` is officially in or out of PR-gated scope.
- A failing check depends on unavailable signing secrets, external credentials, or protected infrastructure.
- A proposed fix would require large-scale generated file churn or architecture changes beyond CI hardening.
- A required check appears fundamentally flaky and cannot be stabilized within the overnight window.

## Reporting Format During The Run

Each iteration should report:

- what failed
- what was changed
- what was re-run
- current remaining blockers

If the run must stop before completion, leave:

- the last green checks
- the remaining red checks
- the exact next action

## First Expected Tasks

The first implementation steps should be:

1. Fix the non-`Send` Rust bootstrap/autonomy path.
2. Re-run Rust compile/test gates.
3. Fix dashboard lint targeting for `dashboard/scripts/**/*.mjs`.
4. Re-run Node required gates.
5. Expand `ci.yml` to cover extension build/typecheck and desktop smoke build.
6. Run the full required set again.
