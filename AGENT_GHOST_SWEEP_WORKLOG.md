# Agent Ghost Sweep Worklog

## 2026-03-23 11:06 EDT

Checked
- `cargo check --manifest-path src-tauri/Cargo.toml` passed.
- `cargo test --manifest-path src-tauri/Cargo.toml` passed (`9` tests passed).
- `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, and `pnpm --dir extension typecheck` were blocked because this worktree has no `node_modules`.
- `pnpm install --offline` also failed because the local pnpm store is missing required tarballs, starting with `@codemirror/lang-markdown@6.5.0`.

Fixed
- Rewired the extension popup script to the actual popup DOM so score, level badge, session duration, alert banner, and signal rows can render again.
- Normalized extension agent loading to accept the gateway's current `/api/agents` array payload and derive a usable display state from `effective_state`/`safety_state`/`lifecycle_state`/`status`.
- Switched popup agent rendering from `innerHTML` string assembly to DOM node creation.

Still broken or unverified
- Dashboard and extension JS checks, builds, and Playwright flows remain unverified until dependencies are installed in this worktree.
- Browser smoke tests could not run for the same reason.
- The extension popup still only renders a derived score from `GET_SCORE`; it does not yet consume richer live score payloads from the background worker.

Next highest-value issue
- Restore JS dependencies in the workspace, then run targeted dashboard and extension validation to catch the next real user-visible breakage, starting with Playwright auth/session and agents flows.
