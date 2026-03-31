# Agent Ghost Category Coverage

Last updated: 2026-03-30

## Category Status

| Category | Status | What was checked | Notes / blockers |
| --- | --- | --- | --- |
| Dashboard UI | in progress | startup/auth boot flow, websocket fan-out, notifications, overview, agents, convergence, costs, trace observability | Frontend package verification is blocked in this worktree because `dashboard/node_modules` is absent. |
| End-to-end flows | partially inspected | login/logout websocket reconnect path, route-level retry flows | Full Playwright execution blocked by missing frontend dependencies. |
| Tauri desktop integration | partially inspected | prior-run desktop notification permission handling and runtime interactions | Reconfirm in a future run once frontend deps are present. |
| Extension behavior | partially inspected | prior-run auth hydration, reconnect auto-sync, observer attach timing, stable platform ids | Reconfirm with package checks when dependencies exist. |
| Error/loading/empty states | in progress | overview, agents, convergence, costs, traces, retry/error recovery paths | Continue sweeping remaining routes. |
| Build and typecheck health | blocked | package scripts and local dependency presence | `dashboard/node_modules` and `extension/node_modules` are missing in this worktree. |
| Runtime/console issues | in progress | websocket leader-election reconnect behavior, notification storage parsing, boot-time listener cleanup | Continue with remaining dashboard stores/components next run. |

## Current Run: Dashboard UI

Checked:

- Layout boot sequence for auth/session verification, websocket startup, PWA install prompt handling, service worker registration, and theme application.
- Shared websocket store reconnection and leader-election behavior for web tabs.
- Notification panel event parsing, persistence safety, and desktop notification dispatch.
- Overview, Agents, Convergence, Costs, and Trace Observability routes for retry behavior, stale state, and empty/error recovery.
- Costs store lifecycle cleanup on route leave.

Completed fixes this run:

1. Recreated the missing persistent category coverage log in-repo.
2. Fixed websocket reconnect teardown to rebuild leader-election state after logout/login.
3. Fixed websocket disconnect to reset stale error and reconnect counters.
4. Fixed websocket disconnect to close and null the broadcast channel consistently.
5. Fixed follower tabs getting stuck with stale leader-election state after reconnect.
6. Fixed follower tabs surfacing leader-only websocket errors in UI state.
7. Fixed layout theme boot applying stale `.light` classes across theme changes.
8. Fixed layout install-prompt boundary using `any` instead of a typed event shape.
9. Fixed layout generic session-verification failure continuing into websocket startup.
10. Fixed layout generic session-verification failure continuing into shortcut registration.
11. Fixed layout generic session-verification failure continuing into push subscription attempts.
12. Fixed layout missing cleanup for the `online` listener.
13. Fixed layout missing cleanup for the `offline` listener.
14. Fixed layout missing cleanup for the `beforeinstallprompt` listener.
15. Fixed layout online transition not updating the recorded last-sync time.
16. Fixed auth-reset redirect not awaiting navigation before returning from boot.
17. Fixed notification id generation assuming `crypto.randomUUID()` always exists.
18. Fixed notification storage parsing accepting malformed non-array payloads.
19. Fixed notification storage parsing accepting malformed item shapes.
20. Fixed notification panel using brittle `any` casts for `AgentStateChange`.
21. Fixed notification panel generating broken `/agents/undefined` links.
22. Fixed notification panel using brittle `any` casts for `KillSwitchActivation`.
23. Fixed notification panel using brittle `any` casts for `InterventionChange`.
24. Fixed notification panel using brittle `any` casts for `ProposalUpdated`.
25. Fixed notification panel navigating without explicitly voiding the async router promise.
26. Fixed overview page retry forcing a full app reload instead of retrying the fetch.
27. Fixed overview page not clearing stale errors before retry.
28. Fixed overview page preserving stale agent counts after failed loads.
29. Fixed overview page preserving stale score/level after failed loads.
30. Fixed overview page not restoring loading state on subsequent refreshes.
31. Fixed agents page retry forcing a full app reload instead of retrying the fetch.
32. Fixed agents page not restoring loading state on subsequent refreshes.
33. Fixed agents page preserving stale cards after failed loads.
34. Fixed agents page preserving stale convergence scores after failed loads.
35. Fixed agents page websocket refresh handlers dropping async intent on the floor.
36. Fixed convergence page retry forcing a full app reload instead of retrying the fetch.
37. Fixed convergence page not restoring loading state on subsequent refreshes.
38. Fixed convergence page preserving stale selected agent ids after score removal.
39. Fixed convergence page preserving stale history after score fetch failures.
40. Fixed convergence page preserving stale score cards after fetch failures.
41. Fixed convergence page failing to clear selection when no scores remain.
42. Fixed costs page leaking store subscriptions across repeated route mounts.
43. Fixed costs store destroy leaving stale cost data in memory.
44. Fixed costs store destroy leaving stale loading state in memory.
45. Fixed costs store destroy leaving stale error state in memory.
46. Fixed trace observability session refresh not clearing previous error state.
47. Fixed trace observability preserving stale selected sessions when no sessions remain.
48. Fixed trace observability preserving stale spans/count when no sessions remain.
49. Fixed trace observability failing to auto-select a valid session after refresh.
50. Fixed trace observability failing to auto-load traces for the new valid selection.

Next category to examine:

- Dashboard UI continuation, with remaining route/component sweep for error/loading/empty states and runtime warnings.
