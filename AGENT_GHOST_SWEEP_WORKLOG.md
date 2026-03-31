# Agent Ghost Sweep Worklog

## 2026-03-24 11:41 EDT

Checked:
- `pnpm -C dashboard check` could not start because `dashboard/node_modules` is missing and sandboxed network access prevented `pnpm install` from reaching the npm registry.
- `pnpm -C extension typecheck` could not start for the same missing-dependencies reason.
- `cargo check` in [`src-tauri`](/Users/geoffreyfernald/.codex/worktrees/d7e2/agent-ghost/src-tauri) completed successfully.
- Direct code inspection covered the extension auth bootstrap, popup wiring, dashboard auth/session flows, and Tauri crate configuration.

Fixed:
- Extension auth validation now checks `/api/auth/session` instead of `/api/health`, preventing invalid or expired tokens from showing as connected in the popup.
- The extension background service worker now initializes persisted auth state on startup.
- The popup script now targets the DOM that [`popup.html`](/Users/geoffreyfernald/.codex/worktrees/d7e2/agent-ghost/extension/src/popup/popup.html) actually renders, so score, level badge, alert banner, signal list, session timer, platform label, and agent list wiring are no longer pointed at nonexistent element IDs.

Still broken or unverified:
- Dashboard and extension JS checks remain unverified because workspace dependencies are not installed and the sandbox cannot currently fetch them.
- Playwright end-to-end flows were not runnable in this pass for the same dependency/install reason.
- The popup still shows placeholder zeroed signal values because the background score message only returns an aggregate score, not the seven signal components.

Next highest-value issue:
- Restore offline-capable JS dependency availability or provide a cached install path, then run dashboard/extension typecheck, lint, build, and Playwright smoke tests to catch remaining user-visible regressions outside the Rust/Tauri shell.
