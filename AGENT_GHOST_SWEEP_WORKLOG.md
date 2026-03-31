# Agent Ghost Sweep Worklog

## 2026-03-30 17:41 EDT
- Checked:
  - `pnpm audit:architecture:strict`
  - `pnpm --dir dashboard check`
  - `pnpm --dir extension typecheck`
  - `cargo check --manifest-path src-tauri/Cargo.toml` (started; reached dependency compilation, not completed during this run)
- Fixed:
  - Removed a forbidden desktop-only import from the Svelte studio route by moving Tauri window focus subscription behind the shared runtime abstraction.
  - Added `subscribeWindowFocus()` to the runtime contract and implemented it in both [`dashboard/src/lib/platform/tauri.ts`](/Users/geoffreyfernald/.codex/worktrees/0500/agent-ghost/dashboard/src/lib/platform/tauri.ts) and [`dashboard/src/lib/platform/web.ts`](/Users/geoffreyfernald/.codex/worktrees/0500/agent-ghost/dashboard/src/lib/platform/web.ts).
  - Updated [`dashboard/src/routes/studio/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/0500/agent-ghost/dashboard/src/routes/studio/+page.svelte) to use the runtime abstraction for foreground resume sync behavior instead of importing `@tauri-apps/api/window` directly.
- Verified:
  - `pnpm audit:architecture:strict` passes after the fix.
- Remains broken / blocked:
  - Dashboard package checks are blocked because this worktree does not have JS dependencies installed; `svelte-kit` was missing.
  - Extension package checks are blocked because this worktree does not have JS dependencies installed; `tsc` was missing.
  - Tauri desktop validation was only partially exercised because `cargo check` did not finish within the run window.
- Next highest-value issue:
  - Restore/install workspace JS dependencies in this worktree, then run `dashboard` check/build plus Playwright smoke coverage to catch the next user-visible regression.
