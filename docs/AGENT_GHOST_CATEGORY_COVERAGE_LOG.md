# Agent Ghost Category Coverage Log

## Category Sequence

1. Dashboard UI and shell
2. Dashboard end-to-end flows
3. Browser extension
4. Tauri desktop integration
5. Error, loading, and empty states
6. Build and typecheck health
7. Runtime and console issues

## Current Status

### Dashboard UI and shell

- Status: in progress
- Last inspected: 2026-03-30
- Summary: focused on dashboard shell wiring, PWA assets, auth entry/exit flow, and notification settings correctness.
- What was checked:
  - favicon and manifest icon references in the dashboard shell
  - service-worker notification icon paths
  - login form submission path
  - settings page browser-only theme initialization
  - sidebar active-state behavior for nested routes
  - window event listener lifecycle in the root layout
  - notification subscription toggle correctness
- Fixes completed this run:
  - replaced broken `favicon.png` reference with a real SVG favicon
  - added missing dashboard icon assets used by the manifest and notifications
  - rewired manifest icons away from nonexistent PNG files
  - rewired service-worker push notification icon and badge paths
  - rewired notification settings test-notification icon path
  - removed duplicate login submission path triggered by Enter key handling plus form submit
  - ensured login loading state always clears via `finally`
  - moved settings theme initialization to `onMount` to avoid browser-only access during component setup
  - awaited logout navigation after local sign-out cleanup
  - made push enablement depend on successful subscription instead of permission alone
  - made push disablement depend on successful unsubscribe instead of optimistic UI state
  - restored active sidebar highlighting for nested `/agents/*` routes
  - restored active sidebar highlighting for the legacy `/settings/channels` bridge
  - cleaned up root-layout `online`, `offline`, and `beforeinstallprompt` listeners on destroy
- Blockers encountered:
  - `pnpm --dir dashboard check` and `pnpm --dir dashboard lint` are blocked because workspace `node_modules` is absent and offline install cannot fetch missing tarballs
  - `cargo test -q` is blocked by disk exhaustion during linking (`No space left on device`)
- Exit criteria remaining before this category can be marked complete:
  - run dashboard `check`, `lint`, and Playwright once dependencies are available
  - continue inspecting route-level shell pages for rendering regressions and missing states

## Next Category

- Continue `Dashboard UI and shell` until local JS verification can run cleanly, then move to `Dashboard end-to-end flows`.
