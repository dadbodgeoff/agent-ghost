# Agent Ghost Sweep Worklog

## 2026-03-31 09:17 EDT
- Checked prior automation memory and verified this worktree still had stale `npm` launcher wiring in [`/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/src-tauri/tauri.conf.json`](/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/src-tauri/tauri.conf.json) and [`/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/dashboard/playwright.config.ts`](/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/dashboard/playwright.config.ts).
- Fixed desktop and Playwright startup/build commands to use the workspace package manager (`pnpm`) instead of `npm`.
- Hardened the A2A send-task prefilling path in [`/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/dashboard/src/routes/orchestration/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/dashboard/src/routes/orchestration/+page.svelte) so discovered agents that already expose `/.well-known/agent.json` or include trailing slashes no longer generate malformed duplicate URLs.
- Added a Playwright regression in [`/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/dashboard/tests/orchestration.spec.ts`](/Users/geoffreyfernald/.codex/worktrees/d550/agent-ghost/dashboard/tests/orchestration.spec.ts) to keep the normalized A2A target behavior covered.
- Validation:
- `cargo check` in `src-tauri/` passed.
- `pnpm --version` returned `10.28.0`.
- JS validation remains blocked in this worktree because `dashboard/node_modules` and `extension/node_modules` are absent and the environment does not have a warmed install.
- Remaining broken / unverified:
- Dashboard `build`, `check`, `lint`, and Playwright suites were not runnable from this worktree because dependencies are not installed.
- Extension `typecheck`, `lint`, and `build` remain unverified for the same reason.
- Next highest-value issue:
- Restore a usable pnpm install or warm store, then run targeted dashboard checks (`check`, `build`, `test:e2e`) and extension checks to find the next user-visible defect beyond startup wiring.
