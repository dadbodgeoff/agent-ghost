# Agent Ghost Sweep Worklog

## 2026-03-23 18:06:01 EDT

Checked:
- `cargo check --manifest-path src-tauri/Cargo.toml` completed successfully.
- `pnpm install --frozen-lockfile` was attempted to unlock dashboard and extension checks, but failed because the environment could not resolve `registry.npmjs.org`.
- Confirmed `dashboard` and `extension` checks were otherwise blocked by missing `node_modules`.
- Reviewed dashboard Playwright/Tauri wiring plus extension popup/background wiring.

Fixed:
- Switched dashboard Playwright preview startup from `npm` to `pnpm`, and made the local e2e server build before previewing so preview runs against fresh static assets.
- Fixed Tauri dashboard hooks to use `pnpm` and the correct sibling `../dashboard` path from [`src-tauri/tauri.conf.json`](/Users/geoffreyfernald/.codex/worktrees/3482/agent-ghost/src-tauri/tauri.conf.json).
- Fixed extension popup DOM wiring in [`extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/3482/agent-ghost/extension/src/popup/popup.ts) so score, level badge, alert banner, session duration, platform label, and disconnected fallback state now target real elements.
- Changed the popup to initialize auth state before rendering, fetch live convergence scores from the gateway when authenticated, and fall back to the background score if the gateway request fails.
- Initialized extension auth restore and offline sync wiring during service worker startup in [`extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/3482/agent-ghost/extension/src/background/service-worker.ts).

Still broken / blocked:
- Dashboard `check`, `lint`, `build`, and Playwright flows could not be executed locally because workspace dependencies are not installed and network access is unavailable.
- Extension `typecheck`, `lint`, and `build` could not be executed locally for the same reason.

Next highest-value issue:
- Once dependencies are available, run `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, `pnpm --dir dashboard test:e2e`, `pnpm --dir extension typecheck`, and `pnpm --dir extension build` to catch the next real UI/runtime regressions beyond the wiring fixes in this sweep.
