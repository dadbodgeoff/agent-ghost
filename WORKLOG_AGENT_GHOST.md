# Agent Ghost Sweep Worklog

## 2026-03-23

Checked:
- `pnpm --dir dashboard check`
- `pnpm --dir dashboard build`
- `pnpm --dir extension typecheck`
- `pnpm --dir extension build`
- `node --check extension/dist/popup/popup.js`
- `node --check extension/dist/background/service-worker.js`
- `node --check extension/dist/storage/sync.js`
- `cargo check --manifest-path src-tauri/Cargo.toml --offline`

What was fixed:
- Rewired the extension popup to the actual DOM contract in [`/Users/geoffreyfernald/.codex/worktrees/c511/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/c511/agent-ghost/extension/src/popup/popup.ts), so score, level badge, alert banner, signal rows, session duration, sync status, platform label, and agent list now target real elements instead of dead IDs.
- Hydrated popup auth from storage with `initAuthSync()` before rendering connection-dependent state, which fixes the false-disconnected startup path.
- Initialized auth sync and auto-sync in the extension background worker and refreshed auth state on storage changes in [`/Users/geoffreyfernald/.codex/worktrees/c511/agent-ghost/extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/c511/agent-ghost/extension/src/background/service-worker.ts).
- Fixed offline replay correctness in [`/Users/geoffreyfernald/.codex/worktrees/c511/agent-ghost/extension/src/storage/sync.ts`](/Users/geoffreyfernald/.codex/worktrees/c511/agent-ghost/extension/src/storage/sync.ts) so non-2xx `/api/memory` responses are no longer marked as synced and silently dropped.
- Patched the corresponding checked-in `extension/dist` files because package rebuilds are currently blocked by missing `node_modules` and no network access.

What remains broken:
- Dashboard and extension package checks are still blocked in this sandbox because dependencies are not installed and network access prevented `pnpm install`.
- The extension has divergent legacy `.js` sources alongside the TypeScript sources; this sweep fixed the TypeScript-driven `dist` path, but the duplicated source trees still need consolidation.
- Dashboard Playwright smoke checks were not runnable in this pass for the same dependency/bootstrap reason.

Next highest-value issue:
- Consolidate the extension's duplicated `.js` and `.ts` implementations, then run the real extension build and dashboard Playwright suite with dependencies installed to catch any remaining UI/runtime regressions.
