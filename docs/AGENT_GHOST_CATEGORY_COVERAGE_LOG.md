# Agent Ghost Category Coverage Log

## Category Sequence

1. Build and typecheck health
2. Dashboard UI
3. End-to-end flows
4. Extension behavior
5. Tauri desktop integration
6. Error/loading/empty states
7. Runtime/console issues

## Current Status

### Build and typecheck health
- Status: in progress
- Scope checked this run:
  - Repository package script/file consistency for root and `dashboard/`
  - Extension auth/background/popup wiring in `extension/src`
  - Desktop compile health in `src-tauri/`
- High-priority fixes completed this run:
  - Extension auth validation now checks `/api/auth/session` instead of the public health probe
  - Extension auth state now initializes in active contexts and listens for storage changes
  - Extension popup now refreshes connection and agent state when auth settings change
  - Extension gateway client and offline sync now bootstrap auth before making authenticated requests
  - Extension background worker now starts auth sync and reconnect auto-sync on startup
  - Extension popup DOM updates now target the actual popup HTML IDs for score, level, alert, signals, and session timer
- Verification:
  - `cargo check` in `src-tauri/` passed on March 29, 2026
  - Static consistency review completed for root scripts, dashboard scripts, and extension manifests
- Blockers:
  - Frontend package verification is partially blocked in this worktree because `node_modules` is absent, so `pnpm`/`tsc`/Playwright checks cannot run without installing dependencies
- Next focus inside category:
  - Dashboard/frontend JS package verification and source-level type/runtime issues once dependencies are available

### Dashboard UI
- Status: pending
- Planned checks:
  - Route-level data loading, retry/error states, navigation, and responsive layout integrity

### End-to-end flows
- Status: pending
- Planned checks:
  - Login/session restore, websocket reconnect, push/service-worker, and key settings flows

### Extension behavior
- Status: pending
- Planned checks:
  - Content observation, popup state, offline queue replay, and native-host fallback

### Tauri desktop integration
- Status: pending
- Planned checks:
  - Gateway lifecycle controls, terminal PTY, desktop notifications, and bundled runtime assumptions

### Error/loading/empty states
- Status: pending
- Planned checks:
  - Empty dashboards, retry flows, partial data, and unavailable backend handling

### Runtime/console issues
- Status: pending
- Planned checks:
  - Browser console noise, unhandled promise paths, and stale/dead source artifacts
