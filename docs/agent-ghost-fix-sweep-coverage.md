# Agent Ghost Fix Sweep Coverage

Last updated: 2026-03-23

## Category sequence

1. Dashboard UI and shared Svelte components
2. Browser extension popup/background wiring
3. End-to-end dashboard flows and Playwright coverage
4. Tauri desktop integration
5. Error, loading, and empty states
6. Build, lint, and typecheck health
7. Runtime and console issues

## Category status

### Dashboard UI and shared Svelte components

- Status: in progress
- What was checked in this run:
  - App shell lifecycle in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/routes/+layout.svelte`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/routes/+layout.svelte)
  - WebSocket lifecycle/orchestration in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/lib/stores/websocket.svelte.ts`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/lib/stores/websocket.svelte.ts)
  - Notification event rendering in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/components/NotificationPanel.svelte`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/components/NotificationPanel.svelte)
  - Keyboard shortcut rendering in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/lib/shortcuts.ts`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/dashboard/src/lib/shortcuts.ts)
  - Accessibility labels/titles in shared dashboard components and key routes
- Fixes completed this run:
  - Guarded shortcut label rendering against SSR `navigator` access.
  - Reset websocket leader-election state on disconnect so reconnect after logout or route teardown can recreate `BroadcastChannel`.
  - Marked websocket reconnect failures as disconnected instead of leaving stale reconnect state.
  - Added cleanup for online, offline, and `beforeinstallprompt` listeners in the dashboard layout.
  - Replaced untyped deferred PWA prompt state with a typed event shape.
  - Closed the PWA install banner after the prompt resolves, including dismissal.
  - Fixed `AgentStateChange` notifications to read `new_state` instead of a non-existent `status` field.
  - Prevented notification links from being built around missing agent ids.
  - Typed notification payload handling for kill switch, intervention, and proposal events.
  - Hardened notification localStorage hydration against malformed payloads.
  - Fixed literal `{...}` accessibility labels and titles across dashboard surfaces so screen readers receive actual values.
- Blockers:
  - `pnpm --dir dashboard check` cannot run in this worktree because dashboard dependencies are not installed (`svelte-kit: command not found` / missing `node_modules`).

### Browser extension popup/background wiring

- Status: in progress
- What was checked in this run:
  - Auth bootstrap in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/background/auth-sync.ts`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/background/auth-sync.ts)
  - Gateway agent fetch logic in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/background/gateway-client.ts`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/background/gateway-client.ts)
  - Background initialization in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/background/service-worker.ts)
  - Popup rendering in [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/extension/src/popup/popup.ts)
- Fixes completed this run:
  - Hydrated extension auth state on service-worker startup.
  - Normalized `/api/agents` responses that come back as a bare array instead of `{ agents: [...] }`.
  - Mapped agent state from `effective_state`, `status`, or legacy `state`.
  - Removed popup `innerHTML` rendering for agent rows and status messages.
  - Validated popup auth state before rendering instead of trusting cold in-memory defaults.
  - Handled `chrome.runtime.lastError` when requesting score data from the background.
  - Cleared stale alert banners when the convergence level drops below the warning threshold.
  - Initialized the popup session timer immediately instead of leaving it blank for the first minute.
  - Closed synchronous message ports explicitly in the background listener instead of keeping them open unnecessarily.
- Blockers:
  - `pnpm --dir extension typecheck` cannot run in this worktree because extension dependencies are not installed (`tsc: command not found` / missing `node_modules`).

### End-to-end dashboard flows and Playwright coverage

- Status: not started

### Tauri desktop integration

- Status: partially checked
- What was checked in this run:
  - Unit tests for [`/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/src-tauri`](/Users/geoffreyfernald/.codex/worktrees/d752/agent-ghost/src-tauri)
- Result:
  - `cargo test --manifest-path src-tauri/Cargo.toml` passed.

### Error, loading, and empty states

- Status: not started

### Build, lint, and typecheck health

- Status: blocked by missing JS dependencies in this worktree

### Runtime and console issues

- Status: not started

## Next category

- Continue `Dashboard UI and shared Svelte components`, then resume `Browser extension popup/background wiring` if another high-priority wiring defect appears during static review.
