# Agent Ghost Category Coverage Log

Last updated: 2026-03-23

## Category order

1. Build and typecheck health
2. Tauri desktop integration
3. Dashboard UI
4. End-to-end flows
5. Extension behavior
6. Error/loading/empty states
7. Runtime/console issues

## Status

| Category | Status | What was checked this run | Outcome | Next action |
| --- | --- | --- | --- | --- |
| Build and typecheck health | In progress | `pnpm typecheck`, `pnpm --dir dashboard check`, `pnpm --dir extension typecheck`, `cargo check --manifest-path src-tauri/Cargo.toml`, `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` | JS verification was blocked because workspace `node_modules` are missing and offline install could not restore all tarballs. Full Rust compile was blocked by disk exhaustion while recreating `src-tauri/target`. Rust formatting and parse validation succeeded for the edited files; `cargo fmt --check` only reported a pre-existing formatting diff in [`/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/menu.rs`](/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/menu.rs). | Resume after dependency cache or disk capacity is restored. |
| Tauri desktop integration | In progress | Manual inspection of [`/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/commands/gateway.rs`](/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/commands/gateway.rs), [`/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/commands/desktop.rs`](/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/commands/desktop.rs), and [`/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/lib.rs`](/Users/geoffreyfernald/.codex/worktrees/2216/agent-ghost/src-tauri/src/lib.rs) | Fixed repeated gateway start/healthy-path state registration so desktop commands no longer re-manage Tauri state, added mutable cached gateway port state with bounded health probes, clamped zero-sized PTY requests, cleaned terminal sessions out of the registry on exit and close, and added a regression test for zero-sized PTY normalization. | Continue desktop inspection next run, then move to dashboard UI. |
| Dashboard UI | Not started | Not inspected in this run | Deferred while build health and Tauri issues were active. | Inspect after Tauri desktop integration. |
| End-to-end flows | Not started | Not inspected in this run | Deferred while build health and Tauri issues were active. | Inspect after dashboard UI. |
| Extension behavior | Not started | Not inspected in this run | Deferred while build health and Tauri issues were active. | Inspect after end-to-end flows. |
| Error/loading/empty states | Not started | Not inspected in this run | Deferred while build health and Tauri issues were active. | Inspect after extension behavior. |
| Runtime/console issues | Not started | Not inspected in this run | Deferred while build health and Tauri issues were active. | Inspect after error/loading/empty states. |

## Fixes landed this run

1. Prevented repeated `start_gateway` calls from panicking by pre-managing gateway state and mutating it in place.
2. Prevented the already-healthy gateway path from trying to register `GatewayProcess` a second time.
3. Made cached gateway port state mutable so status and UI port reads stay consistent after startup.
4. Reused a short-timeout HTTP client for gateway health checks to avoid long desktop hangs.
5. Normalized zero-column PTY open requests to a minimum width of `1`.
6. Normalized zero-row PTY open requests to a minimum height of `1`.
7. Normalized zero-dimension PTY resize requests before calling the PTY backend.
8. Removed terminal sessions from the in-memory registry when the shell exits.
9. Removed terminal sessions from the in-memory registry when the user explicitly closes them.
10. Added a regression test covering zero-dimension PTY normalization.

## Blockers recorded this run

- `pnpm install --offline` failed because the local pnpm store is incomplete for this workspace.
- Rebuilding `src-tauri/target` exhausts available disk in the current environment, so full `cargo check` could not complete.
