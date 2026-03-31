# Agent Ghost Category Coverage Log

## Sweep Sequence

1. Dashboard UI
2. End-to-end flows
3. Tauri desktop integration
4. Extension behavior
5. Error/loading/empty states
6. Build and typecheck health
7. Runtime and console issues

## Current Status

### Dashboard UI

- Status: In progress
- Last inspected: 2026-03-29
- What was checked this run:
  - Root dashboard layout startup and teardown flow
  - Theme initialization and settings theme switching
  - Web runtime storage, token, replay, and notification helpers
  - Notification panel persistence and ID generation
  - Push notification settings gating
  - Shortcut manager browser safety
  - Auth boundary IndexedDB durability path
- Fixes completed this run:
  - Reset stale light-theme class before applying stored theme.
  - Added cleanup for `online` listeners in the root layout.
  - Added cleanup for `offline` listeners in the root layout.
  - Added cleanup for `beforeinstallprompt` listeners in the root layout.
  - Prevented push auto-subscribe from running when service workers are unavailable.
  - Moved settings theme initialization to `onMount` instead of a reactive browser-only effect.
  - Guarded theme writes when `localStorage` or `document` are unavailable.
  - Guarded system-theme media query access behind `window`.
  - Guarded shortcut initialization when the DOM is unavailable.
  - Guarded shortcut teardown when the DOM is unavailable.
  - Guarded shortcut display formatting when `navigator` is unavailable.
  - Added safe `localStorage` checks in the web runtime.
  - Added safe `sessionStorage` checks in the web runtime.
  - Added fallback ID generation when `crypto.randomUUID()` is unavailable in the web runtime.
  - Prevented replay client ID generation from crashing without browser storage.
  - Prevented replay session epoch reads from crashing without browser storage.
  - Prevented replay session epoch writes from crashing without browser storage.
  - Guarded `window.open` in the web runtime.
  - Guarded notification permission requests in non-browser contexts.
  - Added fallback notification IDs when `crypto.randomUUID()` is unavailable.
  - Validated persisted notification payloads before hydrating panel state.
  - Prevented notification settings from reporting push as enabled without service worker support.
  - Prevented notification toggle flow from running without service worker support.
  - Guarded notification-category preference writes behind `localStorage`.
  - Guarded auth-boundary IndexedDB access when IndexedDB is unavailable.
- Verification:
  - `git diff --check` passed.
  - `pnpm --dir dashboard check` blocked because `dashboard/node_modules` is missing in this worktree.
  - `pnpm --dir dashboard lint` blocked because `dashboard/node_modules` is missing in this worktree.
  - `pnpm --dir dashboard build` blocked because `dashboard/node_modules` is missing in this worktree.
- Blockers recorded in-category:
  - Dashboard package dependencies are not installed locally, so Svelte/typecheck/build verification cannot run in this worktree.
- Next category:
  - Dashboard UI (continue until the category is fully inspected with dependencies available), then end-to-end flows.
