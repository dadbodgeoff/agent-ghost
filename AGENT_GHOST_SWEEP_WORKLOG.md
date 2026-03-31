# Agent Ghost Sweep Worklog

## 2026-03-23 09:03:51 EDT

Checked:
- Repo status and package scripts for the monorepo, `dashboard/`, `extension/`, and `src-tauri/`.
- `pnpm -C dashboard check`
- `pnpm -C dashboard build`
- `pnpm -C extension typecheck`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- Static inspection of the dashboard shell, extension popup, extension auth/gateway wiring, and relevant manifests.
- Lightweight verification with `node --check extension/src/popup/popup.js` and `git diff --check`.

Fixed:
- Rewired the extension popup TypeScript entry so the built extension updates the real popup DOM ids from [`/Users/geoffreyfernald/.codex/worktrees/ed8b/agent-ghost/extension/src/popup/popup.html`](/Users/geoffreyfernald/.codex/worktrees/ed8b/agent-ghost/extension/src/popup/popup.html) instead of writing into nonexistent elements.
- Made the built popup initialize auth from storage before fetching gateway data, so authenticated users can load agent and convergence data instead of falling back to a disconnected-looking popup.
- Switched the built popup to read `/api/convergence/scores` correctly via `score`, `level`, and `signal_scores`, with a `GET_SCORE` fallback when gateway fetches fail.
- Updated the legacy JS popup path to keep the status label and empty alert banner in sync for unpacked/source-manifest usage.

Remains broken or blocked:
- JS package checks could not run because the workspace has no installed `node_modules`.
- Rust/Tauri verification could not run because the disk is effectively full (`143Mi` free), and `cargo check` failed immediately with `No space left on device`.
- I did not run Playwright smoke flows in this pass because the dashboard dependencies are missing.

Next highest-value issue:
- Restore a runnable environment first: free disk space and install workspace dependencies, then run targeted dashboard and extension checks plus Playwright smoke tests to catch the next user-visible regression.
