# Agent Ghost Category Coverage Log

## Sequence

1. Build and typecheck health
2. Tauri desktop integration
3. Dashboard UI
4. End-to-end flows
5. Extension behavior
6. Error/loading/empty states
7. Runtime/console issues

## Current Status

- Build and typecheck health: in progress
- Tauri desktop integration: partially inspected during build-health pass
- Dashboard UI: partially inspected during build-health pass
- End-to-end flows: not started
- Extension behavior: not started
- Error/loading/empty states: not started
- Runtime/console issues: not started

## 2026-03-28 Run

- Active category: build and typecheck health
- Checked:
  - `src-tauri/` command/state wiring for gateway lifecycle and desktop token persistence
  - desktop PTY session lifecycle and cleanup paths
  - dashboard root layout startup/retry/login flows
  - workspace validation entry points in `dashboard/`, `extension/`, and `src-tauri/`
- Fixed:
  - split persisted desktop auth token from the local sidecar gateway token
  - stopped Tauri gateway state from being `manage`d multiple times during restart paths
  - initialized gateway process/port state once during app setup
  - made cached gateway port mutable without re-registering app state
  - prevented desktop auto-start from clobbering the user login token
  - started terminal session IDs at `1` instead of the confusing `0`
  - removed terminal sessions from the in-memory registry on process exit
  - removed terminal sessions from the in-memory registry on explicit close
  - flushed PTY writes so interactive shells do not stall on buffered input
  - added a regression test covering separate desktop auth and gateway tokens
  - prevented duplicate login submissions on Enter in the dashboard login form
  - cleaned up global `online` / `offline` / `beforeinstallprompt` listeners in the root layout
  - changed dashboard landing-page retry to reload data instead of hard-refreshing the app
  - changed agents-page retry to reload data instead of hard-refreshing the app
  - changed convergence-page retry to reload data instead of hard-refreshing the app
- Blockers:
  - `dashboard/` and `extension/` checks could not run because local `node_modules` are absent in this worktree
  - Rust/Tauri compile verification is constrained by extremely low free disk space on the workspace volume
- Next category: Tauri desktop integration
