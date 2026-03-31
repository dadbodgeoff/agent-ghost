# Agent Ghost Category Coverage

Last updated: 2026-03-30

## Status Summary

| Category | Status | What was checked | Notes |
| --- | --- | --- | --- |
| Dashboard UI | In progress | Dashboard shell boot, websocket lifecycle, notification panel links/validation, route retry/loading state recovery, traces and costs store lifecycle | Prior run fixed several shared runtime and shell issues; browser package checks still blocked by missing dashboard dependencies. |
| Extension behavior | In progress | Popup rendering, auth bootstrap, gateway request handling, offline sync wiring, content-script observer lifecycle, SPA navigation handling, compiled extension module specifiers, shipped `dist/` runtime files | This run fixed core user-visible extension issues and aligned tracked built output with source. |
| Build and typecheck health | In progress | `pnpm --dir extension typecheck`, `cargo check`, syntax checks on built extension JS, diff hygiene | JS workspace checks blocked by missing `node_modules`; Rust check blocked by disk-full error while creating `src-tauri/target`. |
| End-to-end flows | Not started | Not yet inspected in this sweep sequence | Queue after extension and build health stabilize. |
| Tauri desktop integration | Not started | Not yet inspected in this sweep sequence | Queue after end-to-end flows unless build blockers are cleared first. |
| Error/loading/empty states | In progress | Shared dashboard recovery states and extension popup empty/error states | Continue opportunistically while working adjacent categories. |
| Runtime/console issues | In progress | Dashboard runtime issues and extension module-loading/runtime paths | Continue after environment blockers are removed and checks can run. |

## Current Category Notes

### Dashboard UI

- Fixed on prior run:
  - websocket leader-election teardown and follower-tab socket error behavior
  - shell theme reset, auth reset redirect timing, and leaked global listeners
  - notification payload validation and broken agent links
  - route retry/loading-state recovery for home, agents, convergence, costs, and traces
- Remaining blocker:
  - `pnpm --dir dashboard check` cannot run in this workspace because `dashboard/node_modules` is absent.

### Extension behavior

- Checked:
  - popup live rendering against [`extension/src/popup/popup.html`](/Users/geoffreyfernald/.codex/worktrees/9bf1/agent-ghost/extension/src/popup/popup.html)
  - background startup and auth persistence in [`extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/9bf1/agent-ghost/extension/src/background/service-worker.ts)
  - gateway request behavior in [`extension/src/background/gateway-client.ts`](/Users/geoffreyfernald/.codex/worktrees/9bf1/agent-ghost/extension/src/background/gateway-client.ts)
  - offline sync in [`extension/src/storage/sync.ts`](/Users/geoffreyfernald/.codex/worktrees/9bf1/agent-ghost/extension/src/storage/sync.ts)
  - DOM observation and SPA navigation handling in [`extension/src/content/observer.ts`](/Users/geoffreyfernald/.codex/worktrees/9bf1/agent-ghost/extension/src/content/observer.ts)
  - built runtime parity in tracked `extension/dist/`
- Fixed this run:
  - popup now targets the real DOM ids used by its HTML and renders signals, alerts, session duration, and platform
  - popup auth bootstrap now initializes from storage before loading agents
  - background auth initializes on worker startup and normalizes stored gateway URLs
  - request helper now handles `Headers`, 204 responses, and non-JSON success bodies safely
  - offline sync now runs at startup and stops on non-OK gateway responses instead of falsely marking events as synced
  - service worker now returns deterministic responses for async message handling and validates payload fields
  - content observer now survives SPA route changes, disconnects stale observers, and reuses the current session id consistently
  - base adapter now observes late-mounted containers and descendant nodes instead of silently missing messages
  - extension TS source now uses browser-valid `.js` relative import specifiers
  - tracked built `dist` output was updated to match the repaired source because local rebuild is blocked

## Active Blockers

- `pnpm --dir extension typecheck` fails because `extension/node_modules` is missing.
- `pnpm --dir dashboard check` fails because `dashboard/node_modules` is missing.
- `cargo check` in [`src-tauri`](/Users/geoffreyfernald/.codex/worktrees/9bf1/agent-ghost/src-tauri) currently fails with `No space left on device` while creating `target/debug/.fingerprint/...`.

## Next Category

Next category to examine after the environment blockers are cleared: End-to-end flows.
