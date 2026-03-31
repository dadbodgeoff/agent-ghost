# Agent Ghost Category Coverage Log

## Scope

Tracks category-by-category inspection for the recurring fix sweep. This log is intentionally not a backlog. It records:

- which category was inspected
- what was checked in that category
- what was fixed in that run
- blockers that prevented safe verification or safe fixes
- which category should be inspected next

## Category Status

| Category | Status | Last run | Notes |
| --- | --- | --- | --- |
| Dashboard UI shell + error/loading/empty states | in progress | 2026-03-29 | Shared layout, overview, agents, convergence, channels, notifications inspected and patched. |
| End-to-end flows | not started | - | Next after dashboard UI stabilization. |
| Tauri desktop integration | not started | - | Pending. |
| Extension behavior | not started | - | Pending. |
| Build and typecheck health | blocked | 2026-03-29 | JS dependencies are not installed in this worktree, so Svelte/eslint checks cannot run yet. |
| Runtime and console issues | not started | - | Pending. |

## 2026-03-29 Run

### Active category

Dashboard UI shell + error/loading/empty states

### What was checked

- Root dashboard layout boot flow, PWA install prompt, online/offline handling, and auth/session startup retry behavior.
- Notification bell mapping from websocket events, persisted notification storage, and invalid payload handling.
- Overview-adjacent high-traffic pages: agents, convergence, and channels.
- Page-level error, loading, and empty-state behavior on failed reloads and stale selections.

### Fixes applied

1. Added explicit typing for the PWA install prompt event instead of using an untyped `any`.
2. Added cleanup for the layout's `online` listener.
3. Added cleanup for the layout's `offline` listener.
4. Added cleanup for the layout's `beforeinstallprompt` listener.
5. Added an in-app retry action for session/bootstrap failures instead of forcing a full page refresh.
6. Disabled the bootstrap retry action while retry is in progress.
7. Updated the offline banner to support an actionable retry affordance.
8. Guarded notification routing so missing `agent_id` falls back to `/agents` instead of generating broken `/agents/undefined` links.
9. Replaced several notification `any` payload reads with a typed payload normalizer.
10. Normalized kill-switch notification reason handling.
11. Normalized intervention-change notification payload handling.
12. Normalized proposal update notification payload handling.
13. Validated persisted notifications before hydrating them from `localStorage`.
14. Limited restored notifications to the configured notification cap.
15. Fixed channel timestamp rendering for invalid dates so the UI shows `Unknown` instead of malformed relative time.
16. Reset the channels page into a loading state before each reload.
17. Cleared stale selected-channel state when channel loading fails.
18. Cleared stale selected-channel state after a successful channel removal.
19. Cleared stale selected-channel state after adding a new channel.
20. Added a user-visible error when trying to create a channel without any agent selected.
21. Added an empty-agent hint in the add-channel form.
22. Cleared stale channel errors before reconnect attempts.
23. Cleared stale channel errors before remove attempts.
24. Guarded channel config rendering when config is absent.
25. Reset the convergence page into a loading state before each reload.
26. Cleared stale convergence errors before refetching.
27. Cleared stale convergence scores after a failed load so the page cannot present old data as current.
28. Re-selected a valid agent when the previously selected convergence subject disappears from the latest score payload.
29. Replaced the convergence page full-page reload retry with an in-app data retry.
30. Removed a dead `{#if true}` branch from the convergence radar card.
31. Reset the agents page into a loading state before each reload.
32. Cleared stale agents-page errors before refetching.
33. Cleared stale agent cards after a failed agent load so stale data is not shown.
34. Cleared stale convergence score map after a failed agent load.
35. Replaced the agents page full-page reload retry with an in-app data retry.

### Blockers

- `pnpm --dir dashboard check` fails because `svelte-kit` is unavailable in this worktree.
- `pnpm --dir dashboard lint` fails because `eslint` is unavailable in this worktree.
- No `node_modules` directory is present anywhere in the repository checkout, so JS package verification is blocked without a dependency install.

### Next category

End-to-end flows
