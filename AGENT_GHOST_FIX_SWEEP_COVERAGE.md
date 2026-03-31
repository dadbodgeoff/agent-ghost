# Agent Ghost Fix Sweep Coverage

Last updated: 2026-03-31T00:13:53Z

## Category Status

| Category | Status | What was checked | Outcome |
| --- | --- | --- | --- |
| Tauri desktop integration | In progress | `src-tauri/src/lib.rs`, `src-tauri/src/commands/gateway.rs`, `src-tauri/src/commands/desktop.rs`, dashboard terminal bridge in `dashboard/src/lib/components/Terminal.svelte`; attempted `cargo check --manifest-path src-tauri/Cargo.toml` | Fixed desktop lifecycle issues; compile verification blocked by disk exhaustion during dependency build. |
| Dashboard UI | Not started | Attempted `pnpm --dir dashboard check` and `pnpm --dir dashboard lint` | Blocked in this worktree because `node_modules` is missing and offline registry access prevented install. |
| End-to-end flows | Not started | Not inspected this run | Deferred until dashboard dependencies are available. |
| Extension behavior | Not started | Attempted `pnpm --dir extension typecheck` and `pnpm --dir extension lint` | Blocked in this worktree because `node_modules` is missing and offline registry access prevented install. |
| Error/loading/empty states | Not started | Not inspected this run | Deferred. |
| Build and typecheck health | In progress | Dashboard, extension, and Tauri verification entry points | Environment blockers found: offline npm resolution and only 147 MiB free disk during Rust build. |
| Runtime/console issues | Not started | Not inspected this run | Deferred. |

## Fixes Completed In This Category

1. Initialized gateway process state once at app startup to avoid runtime state re-registration panics.
2. Initialized gateway port state once at app startup and switched it to mutable cached state.
3. Prevented duplicate sidecar launches when auto-start and manual start race.
4. Stopped returning a misleading `"started"` status and now report actual gateway lifecycle state.
5. Added a `"starting"` desktop gateway status when a managed sidecar exists but health is not ready yet.
6. Cleared managed gateway process state when the sidecar terminates so status does not stick on `"starting"`.
7. Clamped terminal PTY dimensions to at least `1x1` to avoid invalid zero-sized sessions.
8. Started desktop terminal session IDs at `1` instead of `0` for cleaner runtime identifiers.
9. Removed terminal sessions from the registry when the child process exits naturally.
10. Removed terminal sessions from the registry on explicit close to prevent state leaks.
11. Made terminal close idempotent so dashboard teardown does not fail after natural PTY exit.
12. Flushed terminal writes after sending input to reduce dropped/lagged interactive input.
13. Disposed dashboard terminal listeners before closing the PTY to avoid teardown races.
14. Cleared the dashboard PTY handle on terminal exit so stale sessions are not reused.

## Blockers Seen This Run

- `pnpm install --frozen-lockfile` could not restore workspace dependencies because registry access failed with `ENOTFOUND`.
- `cargo check --manifest-path src-tauri/Cargo.toml` failed before reaching source validation because the machine only had about `147 MiB` free and rustc hit `No space left on device`.

## Next Category

Continue Tauri desktop integration once disk space is restored enough to compile the desktop crate. If the environment is still blocked next run, shift to dashboard UI static inspection without dependency-backed verification.
