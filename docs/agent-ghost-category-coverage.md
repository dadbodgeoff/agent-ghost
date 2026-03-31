# Agent Ghost Category Coverage

## Current status

### 2026-03-23
- Active category: `build and typecheck health`
- Status: `in progress`
- Scope checked this run:
  - Extension popup auth/bootstrap flow
  - Extension gateway access permissions
  - Extension background refresh/runtime resilience
  - Dashboard `channels` route null/time/config handling
  - Local JS toolchain execution viability in this sandbox
- Verified this run:
  - `git diff --check`
  - JSON parse of [`/Users/geoffreyfernald/.codex/worktrees/716b/agent-ghost/extension/manifest.chrome.json`](/Users/geoffreyfernald/.codex/worktrees/716b/agent-ghost/extension/manifest.chrome.json)
  - JSON parse of [`/Users/geoffreyfernald/.codex/worktrees/716b/agent-ghost/extension/manifest.firefox.json`](/Users/geoffreyfernald/.codex/worktrees/716b/agent-ghost/extension/manifest.firefox.json)
- Fixes completed in this category:
  1. Hydrated extension auth state before popup/background consumers read it.
  2. Removed popup `innerHTML` rendering for agent rows to close an XSS path.
  3. Fixed false-disconnected popup state after browser/service-worker restart.
  4. Fixed extension sync replay reading stale unauthenticated state before storage hydration.
  5. Cleared stale popup alert banners when convergence level drops.
  6. Added local gateway host permissions to Chrome extension manifest.
  7. Added local gateway host permissions to Firefox extension manifest.
  8. Added `alarms` permission for durable background score refresh.
  9. Replaced background `setInterval` refresh scheduling with `chrome.alarms`.
  10. Retried native messaging connection when the emitter is used after disconnect/startup races.
  11. Stopped dropping IndexedDB writes on fire-and-forget transaction shutdown by awaiting completion and closing the DB.
  12. Hardened channel relative-time rendering against invalid/future timestamps.
  13. Hardened channel config rendering against null/non-object payloads.
  14. Prevented silent no-op channel creation when no agents are available.
  15. Added explicit empty-state guidance for channel creation without registered agents.
  16. Normalized channel agent labels so missing names/ids do not crash or render badly.
  17. Restored `loading` cleanup in `loadChannels()` with `finally`.
- Blockers encountered:
  - `pnpm install --frozen-lockfile` cannot complete in this sandbox because npm registry access is unavailable (`ENOTFOUND`), so Svelte/TypeScript workspace checks could not be executed for this run.
  - Root `pnpm typecheck` is therefore blocked by missing `node_modules` rather than a confirmed repo code error.
- Next category to examine:
  - `error/loading/empty states`
- Notes for next run:
  - Resume from dashboard routes with async loaders first, then extension popup/dashboard empty states.
  - Re-attempt JS typecheck only if dependencies are already present in the environment or the sandbox policy changes.
