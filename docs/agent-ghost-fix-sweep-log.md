# Agent Ghost Fix Sweep Coverage Log

## Category Status

| Category | Status | Checked In This Run | Notes |
| --- | --- | --- | --- |
| Dashboard UI + end-to-end flows | In progress | Shell lifecycle, runtime boundary, studio resume wiring, navigation state, dashboard architecture audit | Source fixes landed; local JS/Playwright verification blocked because `node_modules` is absent in this worktree. |
| Tauri desktop integration | Pending | Not started | Next after dashboard category stabilizes. |
| Browser extension behavior | Pending | Not started | Queue after dashboard and Tauri. |
| Error/loading/empty states | Pending | Not started | Fold back into dashboard once baseline checks are runnable. |
| Build and typecheck health | Pending | Dependency-blocked locally | Revisit when dependencies are installed. |
| Runtime/console issues | Pending | Partial overlap via dashboard shell | Fold into active category when browser checks are available. |

## Current Run Notes

- Bootstrapped the persistent category log for the sweep process.
- Active category: `Dashboard UI + end-to-end flows`.
- Checks run:
  - `python3 scripts/check_dashboard_architecture.py`
  - `python3 scripts/check_ws_contract_parity.py`
  - `python3 scripts/check_openapi_parity.py`
  - `pnpm --filter ghost-dashboard check` (blocked: missing local dependencies)
  - `pnpm --filter ghost-dashboard lint` (blocked: missing local dependencies)
  - `pnpm --filter ghost-dashboard test:e2e -- --project='Desktop Chrome'` (blocked: missing local dependencies)
- Fixed in this run:
  - Removed a direct Tauri window import from [`/Users/geoffreyfernald/.codex/worktrees/8473/agent-ghost/dashboard/src/routes/studio/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/8473/agent-ghost/dashboard/src/routes/studio/+page.svelte) by extending the runtime abstraction.
  - Added `subscribeWindowFocus()` to the shared runtime interface and both platform implementations.
  - Prevented dashboard shell event-listener leaks by registering removable handlers for `online`, `offline`, and `beforeinstallprompt`.
  - Cleared stale theme state before reapplying saved theme preference on boot.
  - Stopped the shell bootstrap path after auth-session verification fails instead of continuing into websocket/push setup on a broken session.
  - Guarded push subscription against browsers that expose `PushManager` without `Notification`.
  - Fixed sidebar active-state logic for nested `agents`, `channels`, `pc-control`, `itp`, `security`, `costs`, and `search` routes.

## Next Category

- Continue `Dashboard UI + end-to-end flows` once dependencies are present, then move to `Tauri desktop integration`.
