# Agent Ghost Category Coverage

## Category sequence
1. Dashboard UI
2. End-to-end flows
3. Tauri desktop integration
4. Extension behavior
5. Error/loading/empty states
6. Build and typecheck health
7. Runtime and console issues

## Status
- `Dashboard UI`: in progress
- `End-to-end flows`: pending
- `Tauri desktop integration`: pending
- `Extension behavior`: pending
- `Error/loading/empty states`: pending
- `Build and typecheck health`: pending
- `Runtime and console issues`: pending

## Dashboard UI
- Checked: runtime boundary compliance, Studio resume wiring, Studio input focus state, login flow, root layout boot/auth lifecycle, install/offline listener cleanup, overview retry state, agents retry state, convergence retry state, desktop notification settings behavior.
- Fixes completed this run:
  - Replaced direct Tauri window import in Studio with a runtime-owned app focus subscription.
  - Added `subscribeAppFocus` to the runtime abstraction and both web/Tauri implementations.
  - Fixed Studio resume sync to use the shared runtime boundary instead of route-local platform branching.
  - Fixed Studio input shortcut context so it only stays active while the editor is focused.
  - Fixed overview retry to re-fetch in place instead of hard reloading the page.
  - Fixed agents retry to re-fetch in place instead of hard reloading the page.
  - Fixed convergence retry to re-fetch in place instead of hard reloading the page.
  - Fixed layout startup so the login route no longer opens websocket/push flows unnecessarily.
  - Fixed layout startup so failed session verification stops boot instead of continuing into a degraded shell.
  - Fixed layout cleanup for online/offline listeners.
  - Fixed layout cleanup for install prompt listeners.
  - Fixed push bootstrap guard so web push setup is skipped when service workers or notifications are unavailable.
  - Fixed login Enter handling so form submission is not double-triggered.
  - Fixed notifications settings to work on desktop via runtime notification APIs instead of showing unsupported.
  - Fixed notifications settings to surface action errors instead of silently failing.
  - Fixed desktop notification test action to use the runtime notification channel.
  - Fixed notification toggle labeling/copy to reflect desktop vs web behavior.
  - Added explicit non-submit button types on touched dashboard controls that should never submit a form.
- Blockers:
  - `pnpm install --frozen-lockfile` could not complete because the worktree has no installed node dependencies and network access to `registry.npmjs.org` is unavailable.
  - Because dependencies are unavailable, `pnpm --dir dashboard check`, `pnpm --dir dashboard lint`, and Playwright coverage could not run in this environment.

## Next category
- Continue `Dashboard UI` after dependencies are available or cached locally; the category is not fully inspected yet.
