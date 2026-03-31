# Agent Ghost Sweep Worklog

## 2026-03-23

Checked:
- Workspace/package layout and clean git state.
- Attempted `pnpm install --frozen-lockfile` but npm fetches failed due offline `ENOTFOUND`.
- Inspected dashboard Playwright coverage, orchestration wiring, extension popup wiring, and local Tauri testability.
- Started `cargo test --locked --manifest-path src-tauri/Cargo.toml` to verify the desktop side with cached Rust dependencies.

Fixed:
- Rewired the shipped extension popup controller in [`/Users/geoffreyfernald/.codex/worktrees/103d/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/103d/agent-ghost/extension/src/popup/popup.ts) to match the real popup DOM.
- Restored score, level badge, alert banner, signal list, session timer, sync status, platform label, and connection indicator updates.
- Initialized extension auth state before gateway reads so popup agent loading is no longer stuck on the default unauthenticated in-memory state.

Still broken / unverified:
- Dashboard, extension, and Playwright checks could not be executed because workspace npm dependencies are not installed and network fetches are blocked.
- Tauri verification is still running separately; final status depends on local Rust cache and system libraries.

Next highest-value issue:
- Verify and then harden the dashboard boot/login flow with executable checks once dependencies are available, especially startup banners and auth/session recovery paths covered by Playwright.
