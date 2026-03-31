# Agent Ghost Sweep Worklog

## 2026-03-24

Checked:
- Monorepo `pnpm typecheck`
- Dashboard `pnpm --dir dashboard check`
- Dashboard `pnpm --dir dashboard build`
- Extension `pnpm --dir extension lint`
- Extension `pnpm --dir extension typecheck`
- Extension `pnpm --dir extension build`
- Static inspection of `dashboard/`, `extension/`, and `src-tauri/` wiring after JS toolchain commands were blocked by missing local dependencies

Fixed:
- Rewired the extension popup script to the actual popup DOM IDs so score, level badge, alert banner, signal list, and session duration can render again
- Restored popup auth initialization so stored gateway credentials are loaded before rendering connection state and agent list
- Updated Tauri `beforeDevCommand` and `beforeBuildCommand` to use `pnpm`, matching the repository workspace manager and dashboard workspace dependency setup

Still broken / blocked:
- JS build, lint, and typecheck commands cannot run in this worktree because `node_modules` is absent
- I could not execute Playwright smoke checks for the same reason

Next highest-value issue:
- Reinstall workspace dependencies and run dashboard Playwright coverage, then inspect the highest-traffic Svelte route for runtime regressions that static review cannot catch
