# Agent Ghost Sweep Worklog

## 2026-03-25

Checked:
- `cargo check` in `src-tauri/` completed successfully.
- `pnpm --dir dashboard check`
- `pnpm --dir dashboard lint`
- `pnpm --dir extension typecheck`
- `pnpm --dir extension lint`
- `pnpm install --frozen-lockfile`

Fixed:
- Rewired the extension popup auth bootstrap in [`/Users/geoffreyfernald/.codex/worktrees/b792/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/b792/agent-ghost/extension/src/popup/popup.ts) to call `initAuthSync()` instead of reading an unhydrated in-memory auth singleton. This allows the popup to reflect stored credentials and make gateway-backed calls in its own execution context.
- Normalized the extension default gateway URL in [`/Users/geoffreyfernald/.codex/worktrees/b792/agent-ghost/extension/src/background/auth-sync.ts`](/Users/geoffreyfernald/.codex/worktrees/b792/agent-ghost/extension/src/background/auth-sync.ts) from `localhost` to `127.0.0.1` so it matches the dashboard and desktop runtime defaults.
- Repaired popup DOM wiring in [`/Users/geoffreyfernald/.codex/worktrees/b792/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/b792/agent-ghost/extension/src/popup/popup.ts) so score, level badge, signal rows, alert banner, session duration, and platform label target the actual popup HTML ids/classes.

Remains broken / blocked:
- Frontend validation is currently blocked because this environment has no installed workspace `node_modules`, and `pnpm install --frozen-lockfile` fails with `ENOTFOUND` against `registry.npmjs.org`.
- Dashboard Svelte checks, dashboard lint, extension typecheck, and extension lint could not be rerun to completion for the same dependency reason.

Next highest-value issue:
- Restore JS dependency availability, then run the dashboard Playwright and Svelte checks. The dashboard layout/auth flows have coverage, but they were not executable in this run and are the next best place to find real user-visible regressions.
