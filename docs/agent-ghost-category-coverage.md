# Agent Ghost Category Coverage Log

## Sequence

1. Build and integration health
2. Dashboard UI
3. End-to-end flows
4. Tauri desktop integration
5. Extension behavior
6. Error, loading, and empty states
7. Runtime and console issues

## Current Status

### Build and integration health

- Status: in progress
- Last inspected: 2026-03-30
- Scope checked this run:
  - `dashboard/playwright.config.ts`
  - `dashboard/src/components/NotificationPanel.svelte`
  - `extension/src/background/auth-sync.ts`
  - `extension/src/background/itp-emitter.ts`
  - `extension/src/background/service-worker.ts`
  - `extension/src/content/adapters/*.ts`
  - `extension/src/content/observer.ts`
  - `extension/src/popup/popup.ts`
  - `extension/src/storage/sync.ts`
  - `src-tauri/`
- Verified this run:
  - `cargo check` in `src-tauri/` passed
- Issues fixed this run:
  - Playwright web server now uses `pnpm` and builds before previewing in clean checkouts.
  - Critical dashboard desktop notifications now request permission before sending.
  - Notification payload handling now avoids invalid routes and malformed local storage hydration.
  - Extension background now restores stored auth state on startup.
  - Extension background now initializes queued-event auto-sync on reconnect.
  - Extension offline fallback now uses the shared pending-event queue instead of an orphaned IndexedDB store.
  - Popup now hydrates auth state from storage before rendering connection status.
  - Extension observers now emit stable platform identifiers instead of full page URLs.
  - Extension observers now reuse one stable session id per page session.
  - Extension adapters now wait for late-mounted chat containers before attaching observers.
- Blockers:
  - Frontend package checks are currently blocked in this worktree because no `node_modules/` are present, so `pnpm --dir dashboard check` and `pnpm --dir extension typecheck` cannot run yet.
- Next category:
  - Dashboard UI
