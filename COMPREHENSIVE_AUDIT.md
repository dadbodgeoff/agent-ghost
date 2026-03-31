# Agent Ghost 50 Fix Sweep Category Coverage

## Sequence

1. Dashboard UI
2. End-to-end flows
3. Tauri desktop integration
4. Browser extension
5. Error/loading/empty states follow-up
6. Build and typecheck health
7. Runtime and console issues

## Category Status

### Dashboard UI

- Status: in progress
- Runs: 2026-03-31
- Checked this run:
  - Root dashboard shell startup, auth bootstrap, theme application, install prompt handling, online/offline banners
  - Overview dashboard page retry and stale-state behavior
  - Login flow duplicate submit and loading-state cleanup
  - Settings theme bootstrap and logout redirect flow
  - Web runtime browser-storage guards for non-browser execution paths
  - WebSocket store disconnect and leader-election reset behavior
  - Command palette agent command population and timer cleanup
  - Channels, convergence, agents, and sessions retry/reload behaviors
- Fixed this run:
  - Cleared leaked global listeners from the root layout on unmount
  - Prevented websocket reuse after token removal/logout
  - Stopped boot from continuing into realtime setup after session verification failure
  - Made theme application idempotent when remounting the layout
  - Converted hard page reload retries into in-app reload functions on overview, convergence, and agents
  - Cleared stale list, score, channel, and agent state on load failures
  - Removed login double-submit on Enter
  - Moved login loading reset into `finally`
  - Moved settings theme bootstrap to `onMount`
  - Awaited logout navigation after local sign-out cleanup
  - Guarded `localStorage`, `sessionStorage`, and `window.open` in the web runtime
  - Reset websocket leader-election bookkeeping during disconnect
  - Initialized command-palette agent data so agent actions can appear without visiting another page first
  - Cleared command-palette debounce timers on teardown
  - Avoided unhandled async reload calls from websocket-driven refresh handlers
  - Normalized async retry/load-more button handlers to avoid dropped promise paths
- Blockers:
  - Local JS verification is blocked because this worktree does not contain installed dashboard dependencies (`eslint`, `vite`, `svelte-kit`, `@playwright/test` not present in `node_modules`)

### End-to-end flows

- Status: pending

### Tauri desktop integration

- Status: pending

### Browser extension

- Status: pending

### Error/loading/empty states follow-up

- Status: pending

### Build and typecheck health

- Status: pending

### Runtime and console issues

- Status: pending

## Next Category

- Dashboard UI
