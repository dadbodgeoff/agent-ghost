# Agent Ghost Sweep Worklog

## 2026-03-23 19:05 UTC

Checked:
- Workspace/package layout and automation memory state.
- Targeted package commands for `dashboard` and `extension`.
- Extension popup, background auth wiring, sync initialization, and Tauri/dashboard integration points.

Fixed:
- Rewired the extension popup to use the actual DOM ids from [`extension/src/popup/popup.html`](/Users/geoffreyfernald/.codex/worktrees/8b08/agent-ghost/extension/src/popup/popup.html), restoring visible score, level badge, alert banner, signal rows, and session duration updates from [`extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/8b08/agent-ghost/extension/src/popup/popup.ts).
- Switched popup auth bootstrap from stale in-memory `getAuthState()` to `initAuthSync()` so the popup validates stored credentials before deciding whether it is connected.
- Initialized extension auth sync and queued-event auto-sync on background worker startup in [`extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/8b08/agent-ghost/extension/src/background/service-worker.ts).

Remains broken / blocked:
- `pnpm --dir dashboard check`
- `pnpm --dir dashboard build`
- `pnpm --dir extension typecheck`
- `pnpm --dir extension build`

These all fail before code validation because the workspace has no installed `node_modules`, and offline install is blocked by a missing cached tarball for `@codemirror/lang-markdown@6.5.0`.

Next highest-value issue:
- Restore dependency availability so dashboard Svelte checks, extension typechecks/builds, and Playwright smoke tests can run. After that, the next likely product-risk area is end-to-end dashboard auth/session behavior under real service-worker and gateway conditions.
