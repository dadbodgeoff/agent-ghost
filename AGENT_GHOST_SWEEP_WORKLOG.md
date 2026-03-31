# Agent Ghost Sweep Worklog

## 2026-03-29 19:03Z

Checked:
- Monorepo/package wiring for `dashboard/`, `extension/`, and `src-tauri/`.
- `pnpm --dir dashboard check`
- `pnpm --dir dashboard build`
- `pnpm --dir extension typecheck`
- `pnpm --dir extension build`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- Static inspection of dashboard Playwright coverage and Codex provider auth flow.

Fixed:
- Corrected the dashboard Codex provider login UX in [`dashboard/src/routes/settings/providers/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/1ace/agent-ghost/dashboard/src/routes/settings/providers/+page.svelte).
- The page no longer always claims that a ChatGPT browser login was opened.
- Success messaging now reflects the actual gateway auth path (`chatgpt`, `api_key`, or existing ChatGPT auth tokens).
- Added a manual "Open login page" fallback link when a ChatGPT auth URL exists but automatic browser opening fails.
- Limited polling to the interactive ChatGPT browser flow instead of polling for non-browser auth modes.

Still broken or blocked:
- Frontend checks could not run because workspace `node_modules` are missing and `pnpm install` cannot reach npm (`ENOTFOUND`).
- `src-tauri` compilation is blocked by local disk exhaustion (`No space left on device`) while writing under `src-tauri/target/debug`.

Next highest-value issue:
- Restore local verification capacity first: free disk space for Rust artifacts and hydrate JS dependencies, then run the dashboard Playwright/auth/provider flows and extension build/typecheck to find the next real product bug.
