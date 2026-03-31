# Agent Ghost 50 Fix Sweep Coverage Log

Last updated: 2026-03-30 16:04:02 EDT

## Category Sequence

1. Dashboard UI
2. End-to-end flows
3. Tauri desktop integration
4. Browser extension behavior
5. Error, loading, and empty states
6. Build and typecheck health
7. Runtime and console issues

## Category Status

| Category | Status | What was checked | Notes / blockers |
| --- | --- | --- | --- |
| Dashboard UI | In progress | App shell startup, auth/login flow, websocket reconnect path, overview page, agents page, convergence page, channels page, memory page, notification panel, confirm dialog, nav highlighting, settings channel redirect path | `pnpm install --frozen-lockfile` could not fetch packages because outbound npm access is blocked in this environment, so `dashboard` lint/check could not be run this pass. Manual code inspection and safe source-level fixes were applied instead. |
| End-to-end flows | Not started | Not inspected yet | Next after dashboard UI is fully inspected |
| Tauri desktop integration | Not started | Not inspected yet | Pending |
| Browser extension behavior | Not started | Not inspected yet | Pending |
| Error, loading, and empty states | Not started | Not inspected yet | Some dashboard-state fixes landed opportunistically during the dashboard pass; category not started as a dedicated sweep |
| Build and typecheck health | Not started | Not inspected yet | Blocked for JS packages until networked install is available or dependencies are vendored locally |
| Runtime and console issues | Not started | Not inspected yet | Pending |

## Dashboard UI Fixes Applied This Run

1. Reset websocket leader-election state on disconnect so reconnects can recreate the `BroadcastChannel`.
2. Prevent login-screen startup from opening app websocket connections before authentication.
3. Prevent login-screen startup from registering service workers before the user enters the app.
4. Prevent login-screen startup from requesting push subscriptions before authentication.
5. Redirect already-authenticated users away from `/login` back to the main dashboard.
6. Clear stale auth state and tokens on auth-reset errors even when the current route is `/login`.
7. Remove leaked `online` event listeners from the app shell on teardown.
8. Remove leaked `offline` event listeners from the app shell on teardown.
9. Remove leaked `beforeinstallprompt` event listeners from the app shell on teardown.
10. Keep the sidebar `Agents` nav item active on nested agent routes.
11. Point settings sub-navigation for Channels at `/settings/channels` instead of skipping the settings route.
12. Remove duplicate login submission caused by Enter key handling on both the input and form submit.
13. Make confirm dialogs respond to Escape consistently via a window-level key handler.
14. Autofocus the confirm dialog container when it opens so keyboard dismissal works immediately.
15. Avoid broken notification deep links when websocket events arrive without an `agent_id`.
16. Fall back to a generated notification id when `crypto.randomUUID()` is unavailable.
17. Ignore malformed notification payloads restored from local storage instead of trusting arbitrary JSON.
18. Show a useful error when adding a channel without any available agent selection.
19. Guard channel detail rendering when `selectedChannel.config` is absent.
20. Replace overview-page hard reload retry with an in-place data reload.
21. Clear stale overview metrics after load failure instead of leaving old values visible.
22. Replace agents-page hard reload retry with an in-place data reload.
23. Clear stale agents/convergence cards after agents-page load failure.
24. Replace convergence-page hard reload retry with an in-place data reload.
25. Clear stale convergence data after convergence-page load failure.
26. Make memory clear-search trigger the async reload intentionally instead of dropping the promise.

## Next Focus

Continue the `Dashboard UI` category and inspect remaining dashboard routes/components that were not covered in this pass, then rerun dashboard validation once local JS dependencies are available.
