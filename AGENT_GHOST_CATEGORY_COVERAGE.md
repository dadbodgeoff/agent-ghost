# Agent Ghost Category Coverage

## Category Status

| Category | Status | Last Run | Notes |
| --- | --- | --- | --- |
| Dashboard shell, theme, auth, push, and retry UX | In progress | 2026-03-24 | Core shell reliability pass completed; route-by-route dashboard inspection still incomplete. |
| Playwright end-to-end flows | Pending | - | Next category after dashboard is fully inspected. |
| Browser extension (`extension/`) | Pending | - | Not inspected yet. |
| Tauri desktop integration (`src-tauri/`) | Pending | - | Not inspected yet. |
| Error/loading/empty states outside dashboard shell | Pending | - | Partially touched via retry fixes only. |
| Build and typecheck health | Pending | - | Blocked locally by missing frontend dependencies. |
| Runtime and console issues | Pending | - | Not inspected yet. |

## Current Category

### Dashboard shell, theme, auth, push, and retry UX

Status: In progress

Checked this run:

1. Verified dashboard workspace structure and confirmed there was no prior category coverage log.
2. Attempted `pnpm --dir dashboard check`, `pnpm --dir dashboard lint`, and Playwright auth-session tests.
3. Confirmed all frontend verification is currently blocked because `dashboard/node_modules` is missing in this workspace.
4. Added a shared dashboard theme helper to keep theme persistence and DOM application consistent.
5. Fixed stale `.light` class handling when switching back to dark mode.
6. Fixed stale `.light` class handling for system mode on dark OS preference.
7. Made settings theme initialization safe when browser storage is unavailable.
8. Made command-palette theme toggling reuse the same persisted theme logic as the shell.
9. Guarded theme toggling against non-browser execution.
10. Guarded web runtime base URL resolution against missing `localStorage`.
11. Guarded web runtime replay client ID resolution against missing `localStorage`.
12. Guarded web runtime replay session epoch resolution against missing `localStorage`.
13. Guarded web runtime token reads against missing `sessionStorage`.
14. Guarded web runtime token writes against missing `sessionStorage`.
15. Guarded web runtime token clearing against missing `sessionStorage`.
16. Guarded web runtime external URL opening against missing `window`.
17. Replaced anonymous layout `online` listeners with removable handlers.
18. Replaced anonymous layout `offline` listeners with removable handlers.
19. Replaced anonymous layout install-prompt listeners with removable handlers.
20. Added cleanup for `appinstalled` so the install banner does not linger after installation.
21. Typed the deferred install prompt instead of using `any`.
22. Fixed the install banner so it closes after the native install prompt resolves, even when dismissed.
23. Guarded layout push subscription against browsers without `Notification`.
24. Guarded layout push subscription against browsers without `serviceWorker`.
25. Fixed sidebar active-state highlighting for nested `/agents/*` routes.
26. Hardened notification persistence by rejecting malformed stored payloads instead of trusting arbitrary JSON.
27. Capped restored notifications to the configured in-memory maximum.
28. Guarded notification settings subscription flow against missing service workers.
29. Guarded notification settings unsubscribe flow against missing service workers.
30. Guarded notification category persistence against missing `localStorage`.
31. Guarded test-notification dispatch against registrations that cannot call `showNotification`.
32. Reworked dashboard overview retry to refetch data instead of forcing a full page reload.
33. Reworked agents retry to refetch data instead of forcing a full page reload.
34. Reworked convergence retry to refetch data instead of forcing a full page reload.
35. Ensured overview retry resets loading/error state before refetching.
36. Ensured agents retry resets loading/error state before refetching.
37. Ensured convergence retry resets loading/error state before refetching.
38. Made Studio auth-expiry detection run immediately on mount instead of waiting 60 seconds.
39. Cleared stale Studio expiry warnings when there is no token.
40. Cleared stale Studio expiry warnings when the token payload is malformed.

Verification blockers this run:

- `pnpm --dir dashboard check` failed because `svelte-kit` is unavailable and `dashboard/node_modules` is missing.
- `pnpm --dir dashboard lint` failed because `eslint` is unavailable and `dashboard/node_modules` is missing.
- `pnpm --dir dashboard test:e2e tests/auth-session.spec.ts` failed because `@playwright/test` is unavailable and `dashboard/node_modules` is missing.

Next focus inside this category:

1. Finish route-by-route inspection of dashboard pages not covered in this shell pass.
2. Re-run dashboard checks once dependencies are available.
3. Move to Playwright end-to-end flows after the dashboard category is fully inspected.
