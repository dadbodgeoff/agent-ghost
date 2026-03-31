# Agent Ghost Sweep Worklog

## 2026-03-23

### Checked
- `cargo check` in `src-tauri/` completed successfully.
- Attempted `pnpm install`, `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, and `pnpm --dir extension typecheck`.
- Statically inspected the dashboard shell, login route, extension popup, and extension background worker.

### Fixed
- Rewired the extension popup script to the actual popup DOM ids in [`/Users/geoffreyfernald/.codex/worktrees/aa91/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/aa91/agent-ghost/extension/src/popup/popup.ts), restoring score, level badge, alert banner, signal list, platform label, and session duration rendering.
- Hardened popup agent rendering by escaping gateway-provided agent names and states before writing HTML in [`/Users/geoffreyfernald/.codex/worktrees/aa91/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/aa91/agent-ghost/extension/src/popup/popup.ts).
- Restored extension startup auth hydration and reconnect autosync in [`/Users/geoffreyfernald/.codex/worktrees/aa91/agent-ghost/extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/aa91/agent-ghost/extension/src/background/service-worker.ts).

### Remains Broken Or Unverified
- The JS workspace cannot currently run package checks in this environment because `pnpm install` fails with `ENOTFOUND` fetching npm packages, so dashboard build/check, extension typecheck/build/lint, and Playwright runs remain unverified.
- The Svelte dashboard still needs runtime validation for login, initial data loading, and empty/error-state polish once dependencies are installable.

### Next Highest-Value Issue
- Re-run the dashboard and extension package checks once workspace dependencies are available, then fix the first real dashboard or Playwright failure rather than the current environment-only install blocker.
