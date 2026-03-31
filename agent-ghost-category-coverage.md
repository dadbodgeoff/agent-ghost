# Agent Ghost Category Coverage

Updated: 2026-03-24T04:18:40Z

## Category Status

| Category | Status | Notes |
| --- | --- | --- |
| Dashboard UI and navigation | Partially inspected | Prior run covered route navigation, command palette, deep links, and layout event listeners. |
| Browser extension behavior | Partially inspected | Prior run covered auth/session bootstrap, popup auth loading, content observer capture, and startup sync wiring. |
| Error, loading, and empty states | In progress | This run covered extension popup/auth surfaces and dashboard channels, skills, convergence, costs, ITP, memory detail, and related refresh stores. |
| End-to-end flows | Not started | Wait until frontend dependencies are installed so Playwright can run. |
| Tauri desktop integration | Not started | Wait until Rust dependencies are available locally for meaningful verification. |
| Build and typecheck health | Blocked | `pnpm` dependencies are missing locally. |
| Runtime and console issues | Not started | Best inspected after browser and typecheck verification are available. |

## Current Category: Error, Loading, and Empty States

### Checked This Run

- `extension/src/background/auth-sync.ts`
- `extension/src/background/service-worker.ts`
- `extension/src/popup/popup.ts`
- `dashboard/src/routes/channels/+page.svelte`
- `dashboard/src/routes/skills/+page.svelte`
- `dashboard/src/routes/convergence/+page.svelte`
- `dashboard/src/routes/itp/+page.svelte`
- `dashboard/src/routes/memory/[id]/+page.svelte`
- `dashboard/src/routes/costs/+page.svelte`
- `dashboard/src/lib/stores/agents.svelte.ts`
- `dashboard/src/lib/stores/convergence.svelte.ts`

### High-Priority Fixes Completed This Run

1. Extension auth validation now targets `/api/auth/session` instead of the public health route.
2. Extension service worker now initializes stored auth state on startup.
3. Extension service worker now initializes offline replay sync on startup.
4. Popup score rendering now targets the real score element.
5. Popup level rendering now targets the real badge element and classes.
6. Popup signal rows now render into the existing signal list container.
7. Popup alert banner now targets the real alert element.
8. Popup alert banner now clears correctly when the score drops below warning levels.
9. Popup score fetch now tolerates `chrome.runtime.lastError` instead of failing silently.
10. Popup session duration now renders immediately into the correct element.
11. Popup auth state now initializes from storage before deciding connected vs disconnected UI.
12. Popup agent list now shows a loading state during fetch.
13. Channels add form now shows a loading state while agent options are fetched.
14. Channels add form now shows an explicit agent-load error state.
15. Channels add form now shows a no-agents empty state and disables create until an agent exists.
16. Skills install confirmation now stays open when the install action fails.
17. Skills actions now clear stale error banners before retrying.
18. Convergence retry now reloads in place instead of hard-refreshing the whole app.
19. Convergence reloads now clear stale error state and stale score data before retry.
20. ITP failures now clear stale counters and rows instead of leaving misleading old data visible.
21. ITP empty-state copy now distinguishes “no events yet” from “no events match filters”.
22. Memory detail now exits cleanly with an error when the route is missing an ID.
23. Costs calculations now handle missing spending caps without producing invalid values.
24. Agents store refresh now clears stale errors before a successful reload.
25. Convergence store refresh now clears stale errors before a successful reload.

### Verification

- `git diff --check` passed.
- `pnpm`/Svelte/Playwright verification could not run because frontend dependencies are not installed locally.
- `cargo` verification for `src-tauri` remains blocked offline because required crates are not cached locally.

## Next Category

- Continue `error, loading, and empty states` until the remaining dashboard routes and extension edge cases have been inspected.
- After dependencies are available, move to `end-to-end flows` and verify the fixed surfaces with Playwright before shifting to Tauri.
