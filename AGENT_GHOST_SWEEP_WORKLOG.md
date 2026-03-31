# Agent Ghost Sweep Worklog

## 2026-03-23

### Checked
- Reviewed root, dashboard, extension, and `src-tauri` package scripts and repo layout.
- Ran `pnpm --filter ghost-dashboard check` and confirmed it is currently blocked in this worktree because `dashboard/node_modules` is missing and `svelte-kit` is not installed.
- Ran `pnpm --filter ghost-convergence-extension typecheck` and confirmed it is currently blocked because `extension/node_modules` is missing and `tsc` is not installed.
- Ran `cargo check -p ghost-tauri`, which is currently blocked by offline crate resolution against `static.crates.io`.
- Performed static wiring review across the Svelte dashboard runtime/auth boundary, extension popup/background/auth modules, and Tauri frontend/backend command contracts.

### Fixed
- Rewired extension auth reads to load durable auth state from `chrome.storage.local` before validating or making gateway requests instead of relying on per-context in-memory state.
- Updated extension popup boot to await auth restoration before deciding whether to show `Connected` and before attempting to load agents.
- Updated extension gateway and pending-event sync paths to use the same restored auth state, so popup-driven agent fetches and reconnect syncs do not silently fail when the popup opens in a fresh context.
- Restored background auth initialization on extension service worker startup so the background context hydrates stored credentials immediately.
- Fixed the popup alert banner so high-severity convergence warnings clear when the reported level drops back below the threshold.

### Remains Broken / Blocked
- Dashboard package checks, extension typecheck/build, and Playwright flows are not runnable in this worktree until JS dependencies are installed.
- Tauri verification is not runnable while Cargo cannot resolve crates in the current restricted/offline environment.
- Dashboard user-visible flows still need live browser smoke coverage once dependencies are available, especially login, overview loading, studio reconnect/refresh banners, and settings logout.

### Next Highest-Value Issue
- Once dependencies are available, run `dashboard` build/check/Playwright and inspect the auth/session flows in the Svelte app for real runtime regressions instead of static-only review.
