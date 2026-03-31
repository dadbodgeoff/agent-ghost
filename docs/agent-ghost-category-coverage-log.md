# Agent Ghost Category Coverage Log

## Category Status

| Category | Status | Last run | Notes |
| --- | --- | --- | --- |
| Dashboard UI | In progress | 2026-03-25 | Initial offline source audit completed; runtime and notification defects fixed this run. |
| End-to-end flows | Pending | - | Next category after dashboard UI reaches stable verification coverage. |
| Tauri desktop integration | Pending | - | Not yet inspected in this automation sequence. |
| Extension behavior | Pending | - | Not yet inspected in this automation sequence. |
| Error/loading/empty states | Pending | - | Not yet inspected in this automation sequence. |
| Build and typecheck health | Pending | - | Deferred until frontend dependencies are available or a Rust-side blocker surfaces. |
| Runtime/console issues | Pending | - | Not yet inspected in this automation sequence. |

## Current Category: Dashboard UI

- Run date: 2026-03-25
- Scope checked this run:
  - Runtime abstraction boundaries in the studio route
  - Dashboard startup notification and install-prompt behavior
  - Push-notification settings flow and service-worker assumptions
  - Websocket disconnect/reconnect lifecycle in multi-tab web mode
  - PWA/notification asset references under `dashboard/static`
- Fixes completed this run:
  - Replaced the studio page's direct Tauri window import with a runtime-level app-focus subscription.
  - Refreshed JWT expiry warnings immediately on mount and after foreground resume instead of waiting for the first timer tick.
  - Stopped automatic browser notification permission prompts during dashboard boot; subscription now resumes only when permission is already granted.
  - Added cleanup for global `online`, `offline`, and `beforeinstallprompt` listeners in the root layout.
  - Made push settings detect missing service-worker support instead of presenting a broken toggle.
  - Prevented push settings from showing enabled when browser or gateway subscription registration fails.
  - Prevented push settings from showing disabled when unsubscribe fails.
  - Surfaced user-visible push/test-notification failures instead of silently swallowing them.
  - Reset websocket leader-election state on disconnect so reconnect can recover cleanly after auth/session transitions.
  - Restored missing dashboard icon assets referenced by the manifest, service worker, and test notifications.
- Blockers:
  - `dashboard/node_modules` is not present in this worktree, so Svelte build/typecheck/Playwright verification is blocked offline.
- Verification performed:
  - `python3 scripts/check_dashboard_architecture.py`
  - `python3 scripts/check_ws_contract_parity.py`
  - Static audit of manifest and service-worker icon references
- Next step inside this category:
  - Continue dashboard UI inspection with dependency-backed `svelte-check` and Playwright once the workspace has frontend installs.
