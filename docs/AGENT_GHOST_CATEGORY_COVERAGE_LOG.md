# Agent Ghost Category Coverage Log

This log tracks category-by-category inspection progress for the recurring fix sweep automation. It is intentionally not a backlog of unfixed issues. Blockers are recorded only inside the active category entry for the run where they were encountered.

## Category Sequence

| Order | Category | Status | Last Run | Notes |
| --- | --- | --- | --- | --- |
| 1 | Dashboard UI + build/typecheck health | In progress | 2026-03-26 | Static dashboard sweep completed; local frontend verification blocked by missing workspace `node_modules`. |
| 2 | End-to-end flows (Playwright) | Next | — | Run after dashboard verification can execute locally. |
| 3 | Browser extension behavior | Pending | — | Focus on extension auth, storage, and manifest/runtime wiring. |
| 4 | Tauri desktop integration | Pending | — | Focus on runtime bridge, notification flow, and gateway lifecycle controls. |
| 5 | Error/loading/empty states | Pending | — | Cross-surface UX consistency and recovery flows. |
| 6 | Runtime/console issues | Pending | — | Browser console noise, websocket/runtime recovery, and unhandled errors. |

## Run: 2026-03-26

### Active category

`Dashboard UI + build/typecheck health`

### What was checked

- Dashboard shared startup flow in [`/Users/geoffreyfernald/.codex/worktrees/28ad/agent-ghost/dashboard/src/routes/+layout.svelte`](/Users/geoffreyfernald/.codex/worktrees/28ad/agent-ghost/dashboard/src/routes/+layout.svelte)
- Overview, agents, convergence, channels, and memory routes in `dashboard/src/routes`
- Settings surfaces for theme, notifications, and OAuth provider flows
- Local verification commands:
  - `pnpm --dir dashboard check`
  - `pnpm --dir dashboard lint`
  - `pnpm --dir extension typecheck`

### Fixes completed this run

1. Typed the deferred install prompt event instead of using `any`.
2. Cleared stale light-theme class before reapplying theme state.
3. Split `online` handling into a reusable listener.
4. Split `offline` handling into a reusable listener.
5. Split `beforeinstallprompt` handling into a reusable listener.
6. Added cleanup for the `online` listener on layout teardown.
7. Added cleanup for the `offline` listener on layout teardown.
8. Added cleanup for the install-prompt listener on layout teardown.
9. Updated overview retry to reload data instead of hard-refreshing the whole app.
10. Cleared stale overview score data after dashboard-load failures.
11. Cleared stale overview level data after dashboard-load failures.
12. Cleared stale overview agent data after dashboard-load failures.
13. Reset agent-page error state before reloading.
14. Cleared stale agent cards after agent-load failures.
15. Cleared stale convergence score map after agent-load failures.
16. Updated agents retry to reload route data instead of hard-refreshing.
17. Reset convergence-page error state before reloading.
18. Cleared stale convergence score data after failures.
19. Cleared stale convergence history after failures.
20. Updated convergence retry to reload route data instead of hard-refreshing.
21. Cleared stale channel list after channel-load failures.
22. Cleared stale selected-channel state after channel-load failures.
23. Cleared stale JSON parse errors before revalidating new channel config.
24. Marked async mount loads in channels as intentional fire-and-forget calls.
25. Cleared stale memory-page error state when clearing search filters.
26. Marked memory-page clear-search reload as an intentional async call.
27. Marked memory URL rehydration from `$effect` as an intentional async call.
28. Moved settings theme initialization into `onMount` to avoid browser-only API access during SSR.
29. Applied the stored theme immediately after settings mount hydration.
30. Tightened notifications support detection to require service-worker support.
31. Kept notification preference storage access inside browser-mounted code paths.
32. Switched OAuth connect flow to runtime-aware external URL opening instead of forcing in-app navigation.

### Blockers encountered

- Frontend package dependencies are not installed in this workspace snapshot, so `svelte-kit`, `eslint`, and `tsc` are unavailable from the local package scripts.
- Because of that blocker, this category cannot yet be marked fully inspected; static inspection and targeted code fixes were completed, but local typecheck/lint validation for the dashboard remains incomplete.

### Next category

`End-to-end flows (Playwright)` after the dashboard frontend toolchain is available locally; otherwise continue the dashboard/build-health pass.
