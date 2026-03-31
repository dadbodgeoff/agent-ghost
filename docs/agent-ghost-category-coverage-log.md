# Agent Ghost Category Coverage Log

Updated: 2026-03-31

## Category Sequence
1. Dashboard UI
2. End-to-end flows
3. Browser extension
4. Tauri desktop integration
5. Error/loading/empty states
6. Build and typecheck health
7. Runtime and console issues

## Category Status
| Category | Status | What was checked | Outcome | Next action |
| --- | --- | --- | --- | --- |
| Dashboard UI | in progress | Root layout boot flow, login shell behavior, push subscription gating, studio resume wiring, studio auth-expiry warning path, runtime boundary enforcement via `scripts/check_dashboard_architecture.py` | Fixed login-route websocket/push overreach, fixed unsupported-browser notification guard, removed forbidden direct Tauri import from studio route, centralized resume subscriptions in runtime adapters, fixed JWT base64url decoding for expiry warnings, made expiry warning evaluate immediately on mount | Continue dashboard UI sweep with installed frontend dependencies and live route checks |
| End-to-end flows | pending | Not inspected in this run | Awaiting dashboard UI completion | Inspect Playwright coverage and broken authenticated flows next after dashboard UI stabilizes |
| Browser extension | pending | Not inspected in this run | Awaiting earlier categories | Review popup/service-worker/auth-sync wiring |
| Tauri desktop integration | pending | Not inspected in this run | Awaiting earlier categories | Review command bindings, gateway lifecycle, tray/menu flows |
| Error/loading/empty states | pending | Not inspected in this run | Awaiting earlier categories | Sweep dashboard and extension fallback states |
| Build and typecheck health | pending | Attempted `pnpm install --offline`, `npm run check`, `npm run lint`, `npm run build` | Blocked by missing offline tarball for `@codemirror/lang-markdown@6.5.0`; local node_modules unavailable in this worktree | Re-run after dependencies are hydrated or networked install is available |
| Runtime and console issues | pending | Not inspected in this run | Awaiting earlier categories | Inspect browser/runtime console noise after dependency hydration |

## Run Notes
- Verification completed this run:
  - `python3 scripts/check_dashboard_architecture.py`
  - `git diff --check`
- Tooling blocker encountered:
  - `pnpm install --offline` failed because the local pnpm store does not contain all locked packages, starting with `@codemirror/lang-markdown@6.5.0`.
- No standing backlog is maintained here. Only category coverage, checks performed, and blockers are recorded.
