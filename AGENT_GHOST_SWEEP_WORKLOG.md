# Agent Ghost Sweep Worklog

## 2026-03-31 08:16 EDT

Checked:
- Root workspace metadata and package scripts.
- `dashboard/` package scripts and Playwright config.
- `extension/` package scripts and background/popup wiring.
- `src-tauri/` desktop config and `cargo check`.
- Targeted local commands:
  - `pnpm --dir dashboard check`
  - `pnpm --dir extension typecheck`
  - `pnpm --dir extension build`
  - `pnpm install`
  - `cargo check` in `src-tauri/`

Fixed:
- Normalized desktop dashboard launch wiring in [`/Users/geoffreyfernald/.codex/worktrees/6620/agent-ghost/src-tauri/tauri.conf.json`](/Users/geoffreyfernald/.codex/worktrees/6620/agent-ghost/src-tauri/tauri.conf.json) from `npm` to `pnpm`, matching the workspace package manager and repo verification docs.
- Normalized Playwright preview boot wiring in [`/Users/geoffreyfernald/.codex/worktrees/6620/agent-ghost/dashboard/playwright.config.ts`](/Users/geoffreyfernald/.codex/worktrees/6620/agent-ghost/dashboard/playwright.config.ts) from `npm` to `pnpm`, preventing e2e startup from diverging from the workspace toolchain.

Remains broken or blocked:
- JS package checks are currently blocked by offline dependency installation failures against `registry.npmjs.org`; `pnpm install` cannot complete in this environment, so dashboard Svelte checks, extension typecheck/build, and Playwright smoke runs could not be executed.
- No additional user-visible dashboard or extension runtime issue was verified beyond the launcher mismatch because the frontend dependency graph could not be installed.

Next highest-value issue:
- Restore dependency availability for the workspace, then run `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, `pnpm --dir dashboard test:e2e`, and `pnpm --dir extension build` to surface the next real dashboard/extension regression.
