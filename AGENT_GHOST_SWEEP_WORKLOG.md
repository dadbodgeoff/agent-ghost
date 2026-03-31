# Agent Ghost Sweep Worklog

## 2026-03-30 11:03:09 EDT

Checked:
- Inspected workspace scripts and package layout for `dashboard/`, `extension/`, and `src-tauri/`.
- Attempted `pnpm install`, but npm registry fetches failed with `ENOTFOUND`, so dashboard/extension build, lint, typecheck, and Playwright verification were blocked offline.
- Ran `cargo test --manifest-path src-tauri/Cargo.toml --no-run`.
- Ran `cargo test --manifest-path src-tauri/Cargo.toml`.

Fixed:
- Rewired the extension popup to use background messages instead of reading isolated in-memory auth state directly.
- Aligned [`extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/abba/agent-ghost/extension/src/popup/popup.ts) with the actual DOM in [`extension/src/popup/popup.html`](/Users/geoffreyfernald/.codex/worktrees/abba/agent-ghost/extension/src/popup/popup.html), restoring score, level badge, alert banner, session timer, signal bars, agent list, and sync status updates in the built extension.
- Added `GET_STATUS` and `GET_AGENTS` handlers plus startup initialization for auth sync and offline replay in [`extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/abba/agent-ghost/extension/src/background/service-worker.ts).
- Hardened extension auth cleanup in [`extension/src/background/auth-sync.ts`](/Users/geoffreyfernald/.codex/worktrees/abba/agent-ghost/extension/src/background/auth-sync.ts) so clearing auth also resets validation state and gateway URL.

Remaining broken or unverified:
- Dashboard package checks, extension TypeScript build, and Playwright flows are still unverified because dependencies could not be installed from npm in this environment.
- Extension auth ingestion is still incomplete at the product level: the service worker can restore and validate stored credentials, but there is still no confirmed end-to-end path from dashboard login into extension storage.

Next highest-value issue:
- Once npm access is available, run the extension build/typecheck and dashboard checks first, then validate a real popup + service-worker flow and dashboard smoke path in Playwright.
