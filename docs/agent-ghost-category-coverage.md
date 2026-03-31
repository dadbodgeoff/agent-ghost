# Agent Ghost Category Coverage Log

This log tracks category-by-category inspection coverage for the recurring Agent Ghost fix sweep. It is intentionally not a backlog. Record what was inspected, what was fixed in-run, any hard blockers encountered during verification, and the next category to inspect.

## Status

| Category | Status | What was checked | Notes |
| --- | --- | --- | --- |
| Build and typecheck health | In progress | `src-tauri` compile/test path, desktop runtime lifecycle, gateway state management, repo-local validation command availability | JS workspace validation is currently blocked because `dashboard/` and `extension/` dependencies are not installed in this worktree (`node_modules` missing). |
| Dashboard UI | Pending | Not inspected in this run yet | Queue after JS dependencies are available. |
| End-to-end flows | Pending | Not inspected in this run yet | Queue after dashboard dependencies are available. |
| Tauri desktop integration | In progress | Sidecar startup/shutdown flow, tray/window behavior, PTY session lifecycle | Continued as part of build/typecheck health because it is directly verifiable without JS dependencies. |
| Extension behavior | Pending | Not inspected in this run yet | Queue after JS dependencies are available. |
| Error/loading/empty states | Pending | Not inspected in this run yet | Queue after dashboard dependencies are available. |
| Runtime/console issues | Pending | Not inspected in this run yet | Queue after dashboard dependencies are available. |

## 2026-03-31 Run

- Active categories: `build and typecheck health`, `Tauri desktop integration`
- Checks run:
  - `cargo check --manifest-path src-tauri/Cargo.toml`
  - `cargo test --manifest-path src-tauri/Cargo.toml`
  - Attempted `pnpm --dir dashboard check` and `pnpm --dir extension typecheck`
- Fixes completed:
  - Prevented leaked PTY terminal sessions by removing them from the desktop session registry on explicit close and on natural child exit.
  - Started desktop terminal session IDs at `1` instead of `0` to avoid zero-like sentinel handling in host integrations.
  - Initialized gateway state once during app setup instead of trying to re-register Tauri managed state on each start.
  - Persisted the resolved gateway port through a mutable managed state so repeated status/port reads stay coherent after startup.
  - Prevented duplicate sidecar registration across repeated `auto_start` / `start_gateway` calls.
  - Kill and clear the sidecar handle if health probing fails after spawn to avoid leaving a broken child registered as active.
  - Restored hidden/minimized windows when a second desktop instance is launched instead of only attempting focus on a possibly hidden window.
  - Avoided crashing the desktop tray setup when the default window icon is absent; the app now logs and continues.
- Blockers encountered:
  - `dashboard` checks are blocked: `svelte-kit: command not found` and workspace dependencies are missing.
  - `extension` checks are blocked: `tsc: command not found` and workspace dependencies are missing.
- Next category:
  - Resume `build and typecheck health` only if JS dependencies become available in the worktree; otherwise move to repo-local `runtime/console issues` in surfaces that can be verified without frontend installs.
