# Agent Ghost Sweep Worklog

## 2026-03-31 14:15 EDT

Checked:
- Repo state and automation memory.
- Root, dashboard, extension, and Tauri package wiring.
- Targeted commands: `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, `pnpm --dir extension typecheck`, `pnpm --dir extension build`, `cargo check` in `src-tauri/`.
- Playwright and Tauri config files for broken dev/build paths.

Fixed:
- Updated Tauri dashboard hooks in [`/Users/geoffreyfernald/.codex/worktrees/a7bb/agent-ghost/src-tauri/tauri.conf.json`](/Users/geoffreyfernald/.codex/worktrees/a7bb/agent-ghost/src-tauri/tauri.conf.json) from `npm` to `pnpm --dir ../dashboard ...`, which matches the workspace-based dashboard dependency setup.
- Updated Playwright web server wiring in [`/Users/geoffreyfernald/.codex/worktrees/a7bb/agent-ghost/dashboard/playwright.config.ts`](/Users/geoffreyfernald/.codex/worktrees/a7bb/agent-ghost/dashboard/playwright.config.ts) from `npm run preview` to `pnpm exec vite dev --host 127.0.0.1 --port 4173`, removing a fragile dependency on a prebuilt dashboard artifact and aligning the test runner with the repo package manager.

Still broken / blocked:
- JS package checks cannot run in this worktree because local `node_modules` are missing, so `vite`, `svelte-kit`, and `tsc` are unavailable.
- `cargo check` is currently constrained by disk pressure; it failed while writing temp / target artifacts with `No space left on device`.
- No browser smoke run was possible in this pass because the dashboard toolchain is not installed locally.

Next highest-value issue:
- Restore enough free disk and install workspace dependencies, then run dashboard build/check plus Playwright smoke tests to surface the next real user-visible defect in the Svelte app.
