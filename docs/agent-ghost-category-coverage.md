# Agent Ghost Category Coverage Log

## Sequence

1. Build and typecheck health
2. Dashboard UI
3. End-to-end flows
4. Error/loading/empty states
5. Extension behavior
6. Tauri desktop integration
7. Runtime and console issues

## Status

| Category | Status | Last reviewed | What was checked | Outcome | Next action |
| --- | --- | --- | --- | --- | --- |
| Build and typecheck health | In progress | 2026-03-22 | Dashboard package scripts, startup wiring, retry flows, push notification lifecycle, websocket/login transition wiring, notification storage resilience | 50 issue-level fixes landed in dashboard startup/recovery/push flows. Full `pnpm --filter ghost-dashboard check/lint/build` verification was blocked because local `node_modules` are absent. | Re-run this category first once dependencies are installed locally, then continue with dashboard UI. |
| Dashboard UI | Not started | — | — | — | Inspect after build/typecheck verification is available. |
| End-to-end flows | Not started | — | — | — | Inspect after dashboard UI. |
| Error/loading/empty states | Not started | — | — | — | Inspect after end-to-end flows. |
| Extension behavior | Not started | — | — | — | Inspect after error/loading/empty states. |
| Tauri desktop integration | Not started | — | — | — | Inspect after extension behavior. |
| Runtime and console issues | Not started | — | — | — | Inspect after Tauri desktop integration. |

## 2026-03-22 Build And Typecheck Health Sweep

Checks attempted:

- `pnpm --filter ghost-dashboard check`
- `pnpm --filter ghost-dashboard lint`
- `pnpm --filter ghost-dashboard build`
- `git diff --check`
- source inspection of edited dashboard Svelte files

Verification blocker:

- Dashboard dependencies are not installed in this worktree. `svelte-kit`, `eslint`, and `vite` were unavailable, so package-backed validation could not run.

Fixes completed in this sweep:

1. Typed the deferred PWA install prompt instead of using `any`.
2. Added cleanup for the global `online` listener.
3. Added cleanup for the global `offline` listener.
4. Added cleanup for the global `beforeinstallprompt` listener.
5. Stopped websocket connection attempts on the `/login` surface.
6. Added authenticated bootstrap when transitioning from `/login` into app routes.
7. Added websocket disconnect when transitioning from app routes back to `/login`.
8. Prevented unsupported clients from continuing into websocket/session bootstrap after compatibility failure.
9. Made the PWA install banner dismiss after either accept or dismiss.
10. Guarded startup push subscription against missing `serviceWorker` support.
11. Reused or registered the service worker before startup push subscription.
12. Replaced overview-page hard reload retry with in-app retry.
13. Reset overview-page loading state on each retry.
14. Cleared overview-page stale error state on each retry.
15. Replaced agents-page hard reload retry with in-app retry.
16. Reset agents-page loading state on each retry.
17. Cleared agents-page stale error state on each retry.
18. Replaced convergence-page hard reload retry with in-app retry.
19. Reset convergence-page loading state on each retry.
20. Cleared convergence-page stale error state on each retry.
21. Added Escape-key dismissal for the notification panel.
22. Added a safe notification ID fallback when `crypto.randomUUID()` is unavailable.
23. Validated stored notification types before rendering persisted notifications.
24. Validated stored notification severities before rendering persisted notifications.
25. Dropped malformed persisted notifications instead of rendering them.
26. Reset malformed persisted notification roots to an empty saved list.
27. Wrapped notification persistence writes to tolerate storage quota/privacy failures.
28. Avoided generating `/agents/undefined` links from incomplete websocket payloads.
29. Replaced notification agent ID reads with safe typed extraction.
30. Replaced notification proposal ID reads with safe typed extraction.
31. Replaced notification reason/status/change reads with safe typed extraction.
32. Validated saved push-category preferences against the supported category list.
33. Stopped claiming push support when `serviceWorker` support is absent.
34. Checked for an actual existing push subscription before showing the toggle as enabled.
35. Cleared stale notification-settings errors before toggle operations.
36. Only mark push as enabled after subscription succeeds.
37. Reused existing push subscriptions instead of always calling `subscribe()`.
38. Surfaced a specific error when the gateway fails to return a VAPID key.
39. Surfaced a specific error when no service worker registration is available.
40. Surfaced a specific error when the browser returns an incomplete subscription payload.
41. Kept push enabled when unsubscribe fails instead of silently desynchronizing the toggle.
42. Returned explicit success/failure from unsubscribe flow.
43. Surfaced a specific error when test notifications have no service worker available.
44. Surfaced a user-visible error when test notifications fail.
45. Surfaced a user-visible error for general subscribe failure.
46. Surfaced a user-visible error for general unsubscribe failure.
47. Removed reliance on `navigator.serviceWorker.ready` in notification settings flows.
48. Added resilient parsing fallback for invalid saved notification-category arrays.
49. Made runtime availability reactive so route-auth effects can bootstrap after mount.
50. Prevented startup notification rendering from accepting empty title/message payloads from storage.

