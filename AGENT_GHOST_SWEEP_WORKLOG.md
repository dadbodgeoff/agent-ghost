# Agent Ghost Sweep Worklog

## 2026-03-23

Checked:
- Repo/package entrypoints for `dashboard/`, `extension/`, and `src-tauri/`.
- Targeted validation attempts: `pnpm --dir dashboard check`, `pnpm --dir extension typecheck`, `cargo test --manifest-path src-tauri/Cargo.toml`.
- Lightweight verification: `git diff --check`.

Fixed:
- Rewired the extension popup in [`/Users/geoffreyfernald/.codex/worktrees/bc10/agent-ghost/extension/src/popup/popup.ts`](/Users/geoffreyfernald/.codex/worktrees/bc10/agent-ghost/extension/src/popup/popup.ts) so it targets the actual popup DOM ids/classes, renders session duration into the existing field, clears/shows the alert banner correctly, and initializes auth from persisted storage before deciding whether the gateway is connected.
- Initialized extension auth and auto-sync on background startup in [`/Users/geoffreyfernald/.codex/worktrees/bc10/agent-ghost/extension/src/background/service-worker.ts`](/Users/geoffreyfernald/.codex/worktrees/bc10/agent-ghost/extension/src/background/service-worker.ts) so popup and sync paths are not reading an uninitialized in-memory auth state.
- Hardened queued sync in [`/Users/geoffreyfernald/.codex/worktrees/bc10/agent-ghost/extension/src/storage/sync.ts`](/Users/geoffreyfernald/.codex/worktrees/bc10/agent-ghost/extension/src/storage/sync.ts) to rehydrate auth before replay and to stop treating HTTP error responses as successful syncs.

Remains broken or unverified:
- JS package checks are blocked in this worktree because there are no `node_modules` directories and `pnpm install` fails with `ENOSPC`.
- Tauri compile/test is blocked by the same disk-pressure condition; `cargo test` failed creating temp artifacts under `src-tauri/target`.
- The extension still appears to have deeper offline replay drift: `ITPEmitter` writes its own IndexedDB fallback path while `storage/sync.ts` expects queued events in a different store, so actual offline replay is likely still incomplete.

Next highest-value issue:
- Unify the extension’s offline event queue so the fallback writer and replay reader use the same persisted event path, then validate the popup + replay flow with a real extension build once disk pressure is resolved.
