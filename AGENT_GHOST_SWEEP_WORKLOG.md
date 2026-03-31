# Agent Ghost Sweep Worklog

## 2026-03-26

### Checked
- `pnpm --dir dashboard check` failed immediately because this worktree has no `node_modules` (`svelte-kit: command not found`).
- `pnpm --dir extension typecheck` failed immediately because this worktree has no `node_modules` (`tsc: command not found`).
- `cargo check --manifest-path src-tauri/Cargo.toml` failed before compilation with `No space left on device`; the machine had about `171 MiB` free.
- Reviewed cached package logs in [`dashboard/.turbo/turbo-lint.log`](/Users/geoffreyfernald/.codex/worktrees/b5ab/agent-ghost/dashboard/.turbo/turbo-lint.log), [`dashboard/.turbo/turbo-build.log`](/Users/geoffreyfernald/.codex/worktrees/b5ab/agent-ghost/dashboard/.turbo/turbo-build.log), and [`extension/.turbo/turbo-typecheck.log`](/Users/geoffreyfernald/.codex/worktrees/b5ab/agent-ghost/extension/.turbo/turbo-typecheck.log).
- Inspected live wiring in the extension popup, background auth client, gateway client, and dashboard runtime adapters.

### Fixed
- Repaired the extension popup wiring in [`extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/b5ab/agent-ghost/extension/src/popup/popup.ts).
- The popup now initializes auth from persisted storage before requesting gateway-backed data, instead of reading the default in-memory auth state.
- The popup now targets the actual DOM IDs from [`extension/src/popup/popup.html`](/Users/geoffreyfernald/.codex/worktrees/b5ab/agent-ghost/extension/src/popup/popup.html), so score, level badge, alert banner, signal list, session duration, and platform label can render.
- Added safe fallback rendering when the background worker does not return a score.

### Still broken or blocked
- Dashboard lint remains noisy in cached logs because the audit scripts are linted without the right runtime globals; this needs either ESLint environment scoping or script-level fixes.
- This worktree cannot currently run JS package validation because dependencies are not installed locally.
- Tauri validation is blocked by host disk pressure.

### Next highest-value issue
- Bootstrap extension auth and sync state in the background/service-worker path so queued sync and gateway calls do not start from a cold unauthenticated in-memory state after browser restart.
