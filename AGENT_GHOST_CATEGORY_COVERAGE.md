# Agent Ghost Category Coverage

Last updated: 2026-03-23T13:09:39Z

## Sequence
1. Dashboard build/typecheck health
2. Dashboard UI and end-to-end flows
3. Browser extension behavior
4. Tauri desktop integration
5. Error/loading/empty states polish
6. Runtime and console issues

## Status
| Category | Status | Notes |
| --- | --- | --- |
| Dashboard build/typecheck health | in progress | This run focused on high-frequency dashboard UI regressions caused by implicit button submits across route surfaces and shared components. Full JS checks could not run in this worktree because `dashboard/` and `extension/` `node_modules` are missing. |
| Dashboard UI and end-to-end flows | next | Continue by exercising Playwright-covered flows after dependency installation or in a prepared worktree. |
| Browser extension behavior | pending | Not inspected this run. |
| Tauri desktop integration | pending | Not inspected this run. |
| Error/loading/empty states polish | pending | Not inspected this run. |
| Runtime and console issues | pending | Not inspected this run. |

## Current Category Notes
- Checked dashboard shell, settings, channels, skills, orchestration, sessions/replay, security, agents, studio, workflows, memory, PC control, convergence, ITP, and shared dashboard components for implicit-submit action buttons.
- Fixed 50+ user-visible action controls by adding explicit `type="button"` so clicks inside present or future forms do not trigger accidental submits or navigation races.
- Corrected the settings sub-navigation channel link to point to `/settings/channels` instead of jumping out to `/channels`.
- Verified patch integrity with `git diff --check`.
- Could not run `pnpm --dir dashboard check`, `pnpm --dir dashboard lint`, or `pnpm --dir extension typecheck` in this worktree because the local JS toolchain is absent (`node_modules` missing).

## Continue From Here
- Re-run this category once dependencies are available and execute dashboard checks plus Playwright specs.
- Prioritize remaining dashboard controls/components still lacking explicit button types, then move into real end-to-end flow failures.
