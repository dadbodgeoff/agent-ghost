# Category Coverage Log

## Category Status

| Category | Status | Summary | Next |
| --- | --- | --- | --- |
| Dashboard UI | In progress | Shell, login, notifications, websocket lifecycle, and settings auth/provider surfaces inspected and hardened. | Finish remaining dashboard routes/components, then move to end-to-end flows. |
| End-to-end flows | Pending | Not inspected in this run. | Start after dashboard UI is fully inspected. |
| Tauri desktop integration | Pending | Not inspected in this run. | After end-to-end flows. |
| Extension behavior | Pending | Not inspected in this run. | After Tauri integration. |
| Error/loading/empty states | In progress | Improved in login, notifications, and OAuth/settings surfaces. | Continue as part of the remaining dashboard pass. |
| Build and typecheck health | Blocked | `pnpm --filter ghost-dashboard check` and `build` stop immediately because local dashboard dependencies are missing in this worktree. | Re-run once dependencies are installed. |
| Runtime/console issues | In progress | Reduced likely dashboard runtime failures around auth/websocket/browser APIs. | Continue with the next dashboard pass. |

## Current Run

- Run date: 2026-03-29
- Active category: Dashboard UI
- Checks attempted:
  - `pnpm --filter ghost-dashboard check`
  - `pnpm --filter ghost-dashboard build`
  - `git diff --check`
- Check results:
  - `check`: blocked by missing `svelte-kit` binary because local `node_modules` is absent
  - `build`: blocked by missing `vite` binary because local `node_modules` is absent
  - `git diff --check`: passed

## What Was Checked

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/routes/login/+page.svelte`
- `dashboard/src/components/NotificationPanel.svelte`
- `dashboard/src/lib/platform/web.ts`
- `dashboard/src/lib/shortcuts.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/lib/stores/agents.svelte.ts`
- `dashboard/src/lib/stores/audit.svelte.ts`
- `dashboard/src/routes/settings/providers/+page.svelte`
- `dashboard/src/routes/settings/oauth/+page.svelte`

## Fixes Completed This Run

1. Typed the PWA install prompt instead of using `any`.
2. Added cleanup handles for dashboard online listeners.
3. Added cleanup handles for dashboard offline listeners.
4. Added cleanup handles for dashboard install-prompt listeners.
5. Cleared the old theme class before reapplying theme preference.
6. Disconnected websocket state when the auth token is cleared.
7. Awaited navigation on auth-reset redirect.
8. Avoided websocket startup on the `/login` route.
9. Updated reconnect timestamp when connectivity returns.
10. Guarded push setup when `Notification` is unavailable.
11. Guarded push setup when service workers are unavailable.
12. Removed the invalid `banner` role from the nav logo.
13. Added hover preloading to primary dashboard nav links.
14. Added hover preloading to settings subnav links.
15. Corrected the settings channels link to `/settings/channels`.
16. Moved login loading reset into `finally`.
17. Removed redundant Enter handling from the login token input.
18. Disabled token input autocapitalize.
19. Disabled token input autocorrect.
20. Disabled token input spellcheck.
21. Added token input autofocus.
22. Added a typed helper for notification event-field reads.
23. Removed `any`-based agent-state notification mapping.
24. Removed `any`-based kill-switch notification mapping.
25. Removed `any`-based intervention notification mapping.
26. Removed `any`-based proposal notification mapping.
27. Prevented broken agent deep links when `agent_id` is missing.
28. Added Escape-key dismissal for the notification panel.
29. Validated notification storage payload shape before hydrating state.
30. Added `type="button"` to the notification bell.
31. Added dialog semantics to the notification bell via `aria-haspopup`.
32. Added `aria-expanded` state to the notification bell.
33. Added `aria-controls` linkage between bell and panel.
34. Added `type="button"` to the “Mark all read” action.
35. Guarded replay client-id generation when `localStorage` is unavailable.
36. Guarded replay session-epoch reads when `localStorage` is unavailable.
37. Guarded token reads when `sessionStorage` is unavailable.
38. Guarded token writes when `sessionStorage` is unavailable.
39. Guarded epoch writes when `localStorage` is unavailable.
40. Guarded external URL opening when `window` is unavailable.
41. Guarded shortcut display formatting when `navigator` is unavailable.
42. Reset websocket leader-election state on disconnect.
43. Reset websocket reconnect bookkeeping on disconnect-ready reconnects.
44. Switched agents store refresh wiring to the explicit resync hook.
45. Ensured resync-driven agent refreshes preserve async intent with `void`.
46. Switched audit store refresh wiring to the explicit resync hook.
47. Ensured resync-driven audit refreshes preserve async intent with `void`.
48. Switched Codex polling to environment-safe interval APIs.
49. Prevented duplicate Codex login/logout action races.
50. Added in-flight OAuth connect/disconnect guards and clearer action states.

## Next Category

- Continue `Dashboard UI` until the remaining route/component pass is complete and a real Svelte check/build can run with dependencies installed.
