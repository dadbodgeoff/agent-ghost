# Agent Ghost Category Coverage

## Sequence

1. Dashboard UI
2. End-to-end flows
3. Browser extension behavior
4. Tauri desktop integration
5. Error, loading, and empty states
6. Build and typecheck health
7. Runtime and console issues

## Current Status

### Dashboard UI

- Status: in progress
- Started: 2026-03-31
- Checked this run:
  - shared layout startup, auth boot, theme, service worker, install prompt wiring
  - command palette keyboard navigation and command execution flow
  - notification bell persistence and dialog interaction
  - settings theme and push-notification browser-runtime handling
- Completed this run:
  - fixed command palette escape handling inside the focused dialog
  - fixed command palette negative selection index when arrowing through an empty result set
  - fixed command palette async command and navigation execution ordering
  - fixed command palette debounce timer cleanup on teardown
  - fixed layout listener cleanup for online, offline, and install prompt events
  - fixed layout theme initialization so stale light mode does not persist across remounts
  - fixed layout push subscription path to require service worker support before awaiting readiness
  - fixed notification panel storage hydration to ignore malformed persisted records
  - fixed notification panel keyboard-close behavior
  - fixed notification settings mount state to reflect actual push subscription presence
  - fixed notification settings service-worker guards for subscribe, unsubscribe, and test flows
  - fixed settings page browser-global access so it does not rely on localStorage and document during non-browser rendering
- Blockers:
  - `dashboard` and `extension` dependencies are not installed in this worktree, so `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, and `pnpm --dir extension typecheck` cannot run here
- Next focus inside category:
  - continue dashboard route-by-route inspection for retry, error-state, and keyboard interaction defects

## Next Category

- End-to-end flows
