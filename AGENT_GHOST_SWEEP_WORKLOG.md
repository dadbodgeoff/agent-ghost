# Agent Ghost Sweep Worklog

## 2026-03-30

### Checked
- Inspected monorepo scripts plus package-level scripts for `dashboard/`, `extension/`, and `src-tauri/`.
- Attempted `pnpm --filter ghost-dashboard check`; blocked immediately because `dashboard/node_modules` is missing and `svelte-kit` is unavailable.
- Attempted `pnpm --filter ghost-convergence-extension typecheck`; blocked immediately because `extension/node_modules` is missing and `tsc` is unavailable.
- Ran `cargo check --manifest-path src-tauri/Cargo.toml`; passed.
- Performed source inspection on the extension popup/background wiring and dashboard/Playwright config.

### Fixed
- Rewired the extension popup script to the actual popup DOM so score, level badge, session duration, alert banner, signal rows, sync timestamp, and agent list can render correctly.
- Changed popup auth initialization to load from stored credentials with `initAuthSync()` instead of reading uninitialized in-memory auth state from another execution context.
- Replaced MV3 service-worker `setInterval` score refresh with `chrome.alarms`, which survives service-worker suspension and matches the extension lifecycle.
- Added the `alarms` permission to both Chrome and Firefox manifests to support the new refresh path.

### Still Broken / Blocked
- Frontend validation is blocked by missing JavaScript dependencies in this worktree, so dashboard `check`, extension `typecheck`, dashboard build, and Playwright e2e flows could not be executed.
- The dashboard Playwright config depends on a preview server from a built dashboard, so e2e verification remains blocked until the JS workspace is installed and built.

### Next Highest-Value Issue
- Restore/install the JS workspace dependencies, then run `dashboard` check/build and Playwright smoke flows to surface the next user-visible dashboard breakage.
