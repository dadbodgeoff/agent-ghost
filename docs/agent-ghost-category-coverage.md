# Agent Ghost Category Coverage

Last updated: 2026-03-29

## Sequence

1. Dashboard UI and operator surfaces
2. End-to-end flows and Playwright coverage
3. Tauri desktop integration
4. Browser extension behavior
5. Error, loading, and empty states sweep
6. Build and typecheck health
7. Runtime and console issues

## Status

### Dashboard UI and operator surfaces

- Status: In progress
- Scope checked this run:
  - Proposal queue and proposal detail safety guards
  - Channels management form and detail actions
  - Artifact side panel selection and copy flows
  - Skills actions and quarantine resolution UX
  - OAuth and provider auth settings flows
  - Agent creation wizard validation
  - Dashboard overview and agents list retry behavior
- Verified this run:
  - Source-level consistency review across edited Svelte routes and components
  - `git diff --check` planned after edits
  - JS package checks blocked because the worktree does not have `dashboard/node_modules` or `extension/node_modules`
- Fixes completed this run:
  - 1. Replaced queue-level proposal decision throw path with safe validation.
  - 2. Prevented approving proposals with missing lineage metadata from the list view.
  - 3. Prevented rejecting proposals with missing lineage metadata from the list view.
  - 4. Cleared stale queue notices when applying an agent filter.
  - 5. Cleared stale queue notices when clearing an agent filter.
  - 6. Added retry action to proposal list load errors.
  - 7. Shared canonical proposal-status logic between list and detail views.
  - 8. Shared goal decision request construction between list and detail views.
  - 9. Replaced detail-view decision throw path with safe validation.
  - 10. Prevented approving incomplete proposal detail records.
  - 11. Prevented rejecting incomplete proposal detail records.
  - 12. Added explicit warning when proposal detail metadata is incomplete.
  - 13. Added retry action to proposal detail load errors.
  - 14. Removed non-null assertion from channel reconnect action.
  - 15. Removed non-null assertion from channel remove action.
  - 16. Added busy state to channel reconnect actions.
  - 17. Added busy state to channel remove actions.
  - 18. Cleared selected channel after successful deletion.
  - 19. Blocked channel creation when no agent is selected.
  - 20. Disabled the channel agent picker when there are no agents.
  - 21. Disabled channel creation when there are no agents.
  - 22. Added a visible “create an agent first” hint in the channels form.
  - 23. Cleared stale channel errors before reconnect.
  - 24. Cleared stale channel errors before remove.
  - 25. Kept artifact selection in sync when the artifact list changes.
  - 26. Cleared artifact selection when no artifacts remain.
  - 27. Removed non-null assertions from artifact copy handlers.
  - 28. Added stable tabpanel linkage for artifact tabs.
  - 29. Prevented overlapping artifact copy-feedback timers.
  - 30. Cleared skills-page errors before opening install confirmation.
  - 31. Cleared skills-page errors before switching tabs.
  - 32. Replaced missing quarantine revision throw with a user-facing error.
  - 33. Added quarantine dialog placeholder guidance.
  - 34. Routed OAuth connect through runtime external-open handling.
  - 35. Added connect busy state to OAuth providers.
  - 36. Added empty-scope fallback text for OAuth connections.
  - 37. Cleared OAuth connect errors before starting a new connect flow.
  - 38. Improved Codex provider login messaging when auth is already pending.
  - 39. Cleared provider-edit error state on cancel.
  - 40. Enforced at least one bootstrap channel in the agent wizard.
  - 41. Added step-6 validation to the final agent wizard submit path.
  - 42. Reset dashboard overview metrics when convergence data is absent.
  - 43. Reset dashboard overview state on load failure.
  - 44. Replaced dashboard overview hard reload retry with in-page refetch.
  - 45. Cleared dashboard overview errors before refetching.
  - 46. Cleared agents-list errors before refetching.
  - 47. Reset agents list state on load failure.
  - 48. Sorted agents list deterministically by name.
  - 49. Replaced agents-list hard reload retry with in-page refetch.
  - 50. Cleared OAuth connect busy state in all exit paths.
- Blockers:
  - Package-level lint, typecheck, and Playwright verification cannot run in this worktree because frontend dependencies are not installed locally.
- Next focus within category:
  - Continue dashboard UI by sweeping `convergence`, `studio`, and `settings` pages for remaining hard reloads, event-listener cleanup, and empty/error states.

### End-to-end flows and Playwright coverage

- Status: Not started

### Tauri desktop integration

- Status: Not started

### Browser extension behavior

- Status: Not started

### Error, loading, and empty states sweep

- Status: Not started

### Build and typecheck health

- Status: Not started

### Runtime and console issues

- Status: Not started

## Next Category

- Dashboard UI and operator surfaces remains active until the remaining major pages are fully inspected and locally verified.
