# Agent Ghost Category Coverage Log

This log tracks which sweep categories have been inspected, what was checked, blockers encountered during that category, and which category should be examined next. It is intentionally not a backlog of unfixed issues.

## Category Sequence

1. Build and typecheck health
2. Dashboard UI
3. End-to-end flows
4. Browser extension behavior
5. Tauri desktop integration
6. Error, loading, and empty states
7. Runtime and console issues

## Current Status

| Category | Status | Last touched | Notes |
| --- | --- | --- | --- |
| Build and typecheck health | In progress | 2026-03-23 | Frontend package installation is blocked in this environment because npm registry access is unavailable. Static inspection and Rust verification continued. |
| Dashboard UI | Pending | - | Not started in this log. |
| End-to-end flows | Pending | - | Not started in this log. |
| Browser extension behavior | In progress | 2026-03-23 | Inspected while working through frontend build/runtime health because the popup/background path had user-visible failures. |
| Tauri desktop integration | Pending | - | Not started in this log. |
| Error, loading, and empty states | Pending | - | Not started in this log. |
| Runtime and console issues | Pending | - | Not started in this log. |

## 2026-03-23 Run Notes

### Checked

- Workspace dependency state for `dashboard/` and `extension/`
- `dashboard/src/routes/+layout.svelte`
- `extension/src/background/service-worker.ts`
- `extension/src/background/auth-sync.ts`
- `extension/src/background/gateway-client.ts`
- `extension/src/storage/sync.ts`
- `extension/src/popup/popup.ts`
- `extension/src/content/observer.ts`
- `cargo check -p ghost-gateway`

### Fixes Applied

1. Restored extension auth state during background startup instead of leaving the popup on an uninitialized in-memory auth snapshot.
2. Started extension auto-sync during background startup so queued offline events can flush after reconnect.
3. Added guarded bootstrap logging for background auth/sync startup failures.
4. Added explicit error responses for `NEW_MESSAGE` background handling instead of silently failing.
5. Added explicit error responses for `SESSION_START` background handling instead of silently failing.
6. Hydrated popup auth state from storage before reading gateway connection status.
7. Replaced popup agent list `innerHTML` rendering with DOM/text rendering to avoid injecting untrusted agent names into the popup.
8. Marked the popup disconnected when agent loading fails instead of leaving stale connected state visible.
9. Cleared the popup alert banner when convergence drops below the threshold instead of leaving stale warnings onscreen.
10. Rendered the popup session timer immediately instead of leaving it blank for the first minute.
11. Marked the popup disconnected when background score requests fail or return no payload.
12. Made pending-event queue writes wait for IndexedDB transaction completion before returning success.
13. Checked gateway sync responses before marking queued events as synced.
14. Waited for synced-event IndexedDB updates to complete before counting them as flushed.
15. Waited for cleanup cursor work to complete before returning from synced-event cleanup.
16. Rebuilt the TypeScript popup script against the actual popup HTML so score, level, signals, alert, platform, and session timer all target real DOM nodes.
17. Moved popup initialization behind `DOMContentLoaded` so DOM queries do not race script evaluation.
18. Restored popup signal-list rendering with safe DOM node creation instead of relying on mismatched static markup.
19. Restored popup level badge and alert rendering against the live popup DOM.
20. Restored popup platform and session-duration updates against the live popup DOM.
21. Made dashboard theme application deterministic by clearing stale light-mode state before reapplying the saved preference.
22. Cleaned up dashboard online/offline/install event listeners on destroy.
23. Guarded dashboard push subscription flow behind `Notification` and `serviceWorker` availability checks.

### Blockers

- `pnpm install --frozen-lockfile` cannot complete in this environment because registry access to `registry.npmjs.org` fails with `ENOTFOUND`.
- `dashboard` and `extension` type/build checks cannot be fully executed until dependencies are available locally.

### Next Category

Continue `build and typecheck health` first if dependencies are available on the next run. If the environment is still offline, continue with `browser extension behavior`, then move to `dashboard UI`.
