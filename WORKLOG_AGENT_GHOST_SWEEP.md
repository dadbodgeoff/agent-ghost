# Agent Ghost Sweep Worklog

## 2026-03-23 21:05 UTC

Checked:
- Root workspace script surface plus targeted package scripts for `dashboard/`, `extension/`, and `src-tauri/`.
- `cargo check --manifest-path src-tauri/Cargo.toml` completed successfully.
- Attempted `pnpm install --frozen-lockfile`, `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, and `pnpm --dir extension typecheck && pnpm --dir extension build`.

Fixed:
- Rewired the extension popup to the current popup DOM so score, level badge, alert banner, signals, session duration, and platform text can render again in [`/Users/geoffreyfernald/.codex/worktrees/b553/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/b553/agent-ghost/extension/src/popup/popup.ts).
- Bootstrapped popup auth from persisted extension storage via `initAuthSync()` instead of reading an uninitialized in-memory auth singleton, which previously caused false-disconnected UI and broken agent loading in [`/Users/geoffreyfernald/.codex/worktrees/b553/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/b553/agent-ghost/extension/src/popup/popup.ts).
- Hardened IndexedDB sync so queued events are only marked synced after a successful HTTP response and a committed write transaction, and made event queueing wait for IndexedDB commit completion in [`/Users/geoffreyfernald/.codex/worktrees/b553/agent-ghost/extension/src/storage/sync.ts`](/Users/geoffreyfernald/.codex/worktrees/b553/agent-ghost/extension/src/storage/sync.ts).

Remains broken / blocked:
- JS validation is blocked in this environment because workspace dependencies are not installed and registry access fails with `ENOTFOUND` for `registry.npmjs.org`.
- Dashboard Playwright smoke coverage could not be run for the same reason.

Next highest-value issue:
- Once dependencies are available, run the dashboard and extension checks first and then smoke the browser extension popup/service-worker flow in a browser to confirm the popup wiring fix and find the next broken end-to-end path.
