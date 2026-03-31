# Agent Ghost Category Coverage

This log tracks category-by-category inspection for the autonomous fix sweep. It records what was fully inspected, what is currently in progress, what checks or code paths were reviewed, and what category should be examined next.

## Status

| Category | State | Checked | Notes |
| --- | --- | --- | --- |
| Dashboard UI | In progress | Root layout boot/auth flow, push subscription flow, settings notifications, PWA manifest/icon wiring, settings subnav routing | Static inspection completed. Automated dashboard checks were blocked in this worktree because `dashboard/node_modules` is missing and `pnpm install --offline` failed with `ENOSPC`. |
| End-to-end flows | Next | Not started | Run Playwright flows after the workspace has enough free space to hydrate dependencies. |
| Tauri desktop integration | Pending | Not started | |
| Extension behavior | Pending | Not started | |
| Error/loading/empty states | Pending | Not started | |
| Build and typecheck health | Pending | Not started | |
| Runtime/console issues | Pending | Not started | |

## Dashboard UI Sweep Notes

- Removed a stale unused import in the root layout to reduce dashboard lint noise.
- Stopped auto-requesting web push permission during layout boot; web permission prompts now remain user-initiated.
- Added root layout listener cleanup for `online`, `offline`, and `beforeinstallprompt` handlers to avoid duplicate handlers after remounts.
- Registered the service worker before attempting push subscription sync so `navigator.serviceWorker.ready` does not hang behind a failed registration.
- Synced existing push subscriptions back to the backend instead of silently returning when a browser subscription already exists.
- Hardened notification settings support checks so browsers without Service Worker support do not present a broken push toggle.
- Made notification settings derive `pushEnabled` from the actual browser subscription instead of permission state alone.
- Made notification settings roll back the enabled state when subscription setup fails or no VAPID key is available.
- Replaced missing manifest and notification icon references with a checked-in SVG asset.
- Confirmed the settings-level channels route already redirects to the canonical `/channels` surface and left that product direction intact.

## Current Blockers

- `pnpm --dir dashboard check`
  Blocked because dashboard dependencies are not installed in this worktree.
- `pnpm install --offline`
  Blocked with `ENOSPC` in the worktree, so package hydration and follow-up build/typecheck verification could not be completed in this run.

## Next Category

End-to-end flows, once workspace disk pressure is resolved enough to run Playwright and dashboard dependency installation.
