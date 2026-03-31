# Agent Ghost Sweep Worklog

## 2026-03-23

### Checked
- Inspected monorepo scripts in `package.json`, `dashboard/package.json`, `extension/package.json`, and `src-tauri/Cargo.toml`.
- Attempted `pnpm install --frozen-lockfile` to enable dashboard and extension checks.
- Attempted `cargo check -p ghost-desktop` for the Tauri desktop surface.
- Reviewed extension popup, auth, gateway client, background service worker, and offline sync wiring directly in source.

### Fixed
- Repaired extension auth initialization so each extension context hydrates stored gateway credentials before making authenticated requests.
- Fixed popup rendering to target the actual DOM ids in [`extension/src/popup/popup.html`](/Users/geoffreyfernald/.codex/worktrees/4bd4/agent-ghost/extension/src/popup/popup.html), which restores score, level badge, alert banner, session timer, platform label, and signal list rendering.
- Wired background startup to hydrate auth state and enable pending-event auto-sync.
- Tightened background message handling so known messages return cleanly and unknown messages return an explicit error response instead of falling through.

### Still Broken / Blocked
- Node-based dashboard and extension build/typecheck/lint/e2e flows are currently blocked in this environment because `pnpm install` cannot resolve `registry.npmjs.org`.
- Rust/Tauri verification is currently blocked in this environment because `cargo` cannot resolve `static.crates.io`.
- The dashboard and desktop surfaces still need a runtime-backed smoke pass once dependencies are available locally.

### Next Highest-Value Issue
- Validate the extension end-to-end after dependencies are available, then sweep the dashboard/Tauri runtime boundary for similar stale UI-to-runtime wiring mismatches.
