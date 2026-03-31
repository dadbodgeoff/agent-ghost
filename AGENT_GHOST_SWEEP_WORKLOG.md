# Agent Ghost Sweep Worklog

## 2026-03-23 09:05 EDT

Checked:
- Repo/package surface for `dashboard/`, `extension/`, and `src-tauri/`
- Targeted validation commands:
  - `pnpm --dir dashboard check`
  - `pnpm --dir dashboard build`
  - `pnpm --dir extension typecheck`
  - `cargo check` in `src-tauri/`
- Source wiring for dashboard auth/session boot, extension popup/background auth flow, and desktop PTY lifecycle

Fixed:
- Extension popup now initializes auth state from persisted storage before rendering connection state or fetching agents
- Extension popup script now targets the actual popup DOM IDs and renders signal rows, alert banner state, platform hostname, and session duration instead of silently failing against missing elements
- Extension background service worker now bootstraps cached auth state on startup so gateway-facing flows do not begin from a false disconnected default
- Tauri terminal session close path now removes closed PTY sessions from the in-memory registry instead of leaving stale session entries behind

Still broken / blocked:
- JS package checks are currently blocked in this worktree because local dependencies are not installed (`vite`, `svelte-kit`, `tsc` missing from package scripts)
- Rust validation is currently blocked by local disk exhaustion (`cargo check` fails with `No space left on device`)
- Dashboard Playwright smoke coverage was not runnable in this environment because the dashboard toolchain is unavailable without dependencies

Next highest-value issue:
- Restore a runnable local toolchain (`pnpm install` or equivalent workspace dependencies, plus enough free disk for Rust target output), then run dashboard build/check and Playwright auth/session flows to catch remaining user-visible regressions in the Svelte app
