# Agent Ghost Category Coverage Log

## Category Sequence

1. Dashboard UI and build health
2. Playwright end-to-end flows
3. Browser extension behavior
4. Tauri desktop integration
5. Error, loading, and empty states
6. Build, lint, and typecheck health
7. Runtime and console issues

## Current Status

- Active category: Dashboard UI and build health
- State: In progress
- Started: 2026-03-23
- Next category: Playwright end-to-end flows

## This Run Checklist

- Inspect dashboard shell wiring, navigation, theme, notifications, and proposal flows.
- Record local verification blockers when the dashboard toolchain is unavailable.
- Verify any touched PWA assets and browser-runtime assumptions.

## Run Notes

- 2026-03-23: Created the category coverage log and began the dashboard UI/build-health sweep.
- 2026-03-23: Completed a 30-fix dashboard resilience pass covering shell listeners, theme/runtime storage safety, push/notification flows, proposal decision degradation paths, overview retry handling, nested agents nav state, and missing PWA manifest icons.

## What Was Checked

- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/lib/platform/web.ts`
- `dashboard/src/components/NotificationPanel.svelte`
- `dashboard/src/routes/settings/+page.svelte`
- `dashboard/src/routes/settings/notifications/+page.svelte`
- `dashboard/src/routes/+page.svelte`
- `dashboard/src/routes/goals/+page.svelte`
- `dashboard/src/routes/goals/[id]/+page.svelte`
- `dashboard/src/routes/skills/+page.svelte`
- `dashboard/static/manifest.json` and static icon assets

## Fixes Landed In This Run

- Restored manifest icon files so the PWA install surface no longer references missing assets.
- Hardened browser runtime storage access against privacy-mode/storage exceptions.
- Added a fallback client-id generator when `crypto.randomUUID()` is unavailable.
- Added popup-blocked fallback navigation for external auth/login opens.
- Removed stale theme classes before reapplying the saved theme.
- Added live system-theme sync in the dashboard shell.
- Added cleanup for shell `online` listeners.
- Added cleanup for shell `offline` listeners.
- Added cleanup for shell `beforeinstallprompt` listeners.
- Replaced `any` install-prompt state with a typed event shape.
- Guarded push subscription bootstrapping behind notification and service-worker availability.
- Fixed active sidebar state for nested `/agents/*` routes.
- Added notification panel Escape-key close behavior.
- Prevented notification links from routing to `/agents/undefined`.
- Validated and capped persisted notification payloads loaded from local storage.
- Derived notifications settings toggle state from the actual existing push subscription.
- Validated saved notification-category preferences before using them.
- Avoided duplicate push subscriptions when one already exists.
- Guarded push subscribe/unsubscribe paths when service workers are unavailable.
- Kept notification toggle state aligned after subscribe and unsubscribe calls.
- Guarded test-notification sends behind granted notification permission.
- Swapped overview retry from full-page reload to an in-app data reload.
- Prevented skill-quarantine resolution from throwing when the revision is missing.
- Prevented proposal-list decisions from crashing when concurrency metadata is absent.
- Prevented proposal-detail decisions from crashing when concurrency metadata is absent.
- Preserved the existing product surface while improving failure handling around session boot and push setup.

## Verification

- `pnpm --dir dashboard check`
  Blocked locally because this worktree does not have dashboard `node_modules`; `svelte-kit` is not installed.
- Verified static PWA assets exist at `dashboard/static/icons/ghost-192.png`, `dashboard/static/icons/ghost-512.png`, and `dashboard/static/icons/ghost-512-maskable.png`.
- Reviewed edited sources with `git diff --stat` and direct file inspection.

## Blockers

- Local dashboard `check`/`lint` verification is blocked until dependencies are installed in `dashboard/`.
