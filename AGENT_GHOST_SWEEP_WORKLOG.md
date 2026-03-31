# Agent Ghost Sweep Worklog

## 2026-03-25

### Checked
- `pnpm --filter ghost-dashboard check`
- `pnpm --filter ghost-dashboard build`
- `pnpm --filter ghost-convergence-extension typecheck`
- `pnpm --filter ghost-convergence-extension build`
- `pnpm install`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- Dashboard auth/bootstrap and websocket runtime wiring
- Extension popup, background auth sync, gateway client response-shape handling

### Fixed
- Initialized extension auth state in the background service worker so stored gateway credentials are loaded when the extension boots.
- Enabled extension offline replay auto-sync from the background service worker so queued events can flush after connectivity returns.
- Fixed the popup to await auth initialization before rendering connection state or fetching agents.
- Made the extension gateway client accept both the canonical array response and `{ agents: [...] }` wrapper for `GET /api/agents`.
- Reset dashboard websocket leader-election state on disconnect so reconnect after auth/logout teardown can recreate cross-tab listeners cleanly.

### Remains Broken or Blocked
- JavaScript package checks are currently blocked in this sandbox because `node_modules` is absent and `pnpm install` cannot reach `registry.npmjs.org` (`ENOTFOUND`).
- Dashboard Playwright, Svelte check, Vite build, and extension TypeScript validation could not be executed for the same reason.
- Extension content-observer/session telemetry still warrants a real browser smoke test once dependencies are available.

### Next Highest-Value Issue
- Restore package installation in the workspace and run the dashboard Playwright/auth/session suite plus extension build/typecheck to catch runtime regressions that static inspection cannot confirm.
