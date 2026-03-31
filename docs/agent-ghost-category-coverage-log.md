# Agent Ghost Category Coverage Log

## Status
- Active category: Dashboard UI plus error/loading/empty states
- Category state: In progress
- Last updated: 2026-03-30 23:16:28 EDT
- Next category: Extension behavior

## What this run checked
- Dashboard shell bootstrap in [`/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/+layout.svelte`](/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/+layout.svelte)
- Web runtime browser storage and notification access in [`/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/lib/platform/web.ts`](/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/lib/platform/web.ts)
- Notification settings flow in [`/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/settings/notifications/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/settings/notifications/+page.svelte)
- Notification feed handling in [`/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/components/NotificationPanel.svelte`](/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/components/NotificationPanel.svelte)
- Proposal review actions in [`/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/goals/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/goals/+page.svelte) and [`/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/goals/[id]/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/goals/[id]/+page.svelte)
- Skill quarantine resolution flow in [`/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/skills/+page.svelte`](/Users/geoffreyfernald/.codex/worktrees/0b2b/agent-ghost/dashboard/src/routes/skills/+page.svelte)

## Fixes completed this run (50)
1. Guarded localStorage reads for gateway URL override.
2. Guarded localStorage writes for replay client id.
3. Guarded localStorage reads for replay client id.
4. Added random id fallback when `crypto.randomUUID()` is unavailable.
5. Guarded localStorage reads for replay session epoch.
6. Guarded localStorage writes for replay session epoch reset.
7. Guarded sessionStorage reads for auth token.
8. Guarded sessionStorage writes for auth token.
9. Guarded sessionStorage deletes for auth token.
10. Guarded `window.open` for non-window contexts.
11. Cleared stale `light` theme class before applying stored theme.
12. Replaced anonymous online listener with removable handler.
13. Replaced anonymous offline listener with removable handler.
14. Replaced anonymous install-prompt listener with removable handler.
15. Removed online listener during layout teardown.
16. Removed offline listener during layout teardown.
17. Removed install-prompt listener during layout teardown.
18. Updated last-sync timestamp when connection returns online.
19. Stopped auto-prompting browser notification permission at dashboard boot.
20. Guarded boot-time push subscription against missing `Notification` API.
21. Guarded boot-time push subscription against missing service worker support.
22. Guarded boot-time push subscription against missing `PushManager` support.
23. Loaded saved notification categories through validated parsing.
24. Persisted notification categories through guarded storage writes.
25. Detected existing push subscription during notifications page mount.
26. Reflected real subscription state instead of only permission state.
27. Prevented subscribe flow from reporting success when VAPID key is absent.
28. Reused existing push subscriptions instead of always creating a new one.
29. Cleaned invalid existing push subscriptions before re-subscribing.
30. Returned explicit subscribe success/failure to keep toggle state accurate.
31. Returned explicit unsubscribe success/failure to keep toggle state accurate.
32. Prevented test-notification trigger when permission is not granted.
33. Disabled test-notification button unless permission is granted.
34. Added notification id fallback when `crypto.randomUUID()` is unavailable.
35. Removed unsafe `any` access for agent-state notification payloads.
36. Removed unsafe `any` access for kill-switch notification payloads.
37. Removed unsafe `any` access for intervention notification payloads.
38. Removed unsafe `any` access for proposal notification payloads.
39. Sent agent-state notifications to `/agents` when agent id is missing.
40. Avoided malformed notification text when websocket fields are empty.
41. Validated persisted notification payload shape before hydration.
42. Capped restored notifications to the same runtime max limit.
43. Guarded notification storage writes against quota/storage failures.
44. Prevented proposal list actions from throwing on incomplete review metadata.
45. Surfaced a user-facing proposal list error when lineage metadata is missing.
46. Prevented proposal detail actions from throwing on incomplete review metadata.
47. Surfaced a user-facing proposal detail error when lineage metadata is missing.
48. Prevented skill quarantine resolution from throwing when revision is absent.
49. Surfaced a user-facing skill error when quarantine revision is absent.
50. Kept dashboard shell teardown clean instead of accumulating global listeners across mounts.

## Blockers encountered
- Frontend package dependencies are not installed in this worktree, so `pnpm --dir dashboard check`, `pnpm --dir dashboard build`, and `pnpm --dir extension lint` cannot run.
- The temp volume is effectively full for patch tooling, so edits had to be written without `apply_patch`.

## Exit criteria status for this category
- Runtime crash paths from incomplete dashboard data: improved
- Notification and push UX: improved
- Listener lifecycle hygiene: improved
- Full category verification: blocked pending frontend dependency install
