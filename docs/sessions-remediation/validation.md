# Sessions Remediation Validation Doctrine

The Sessions subsystem is not complete until these checks pass.

## Ship Rules

- no route component silently truncates the session universe
- no mutation can succeed without proving session ownership and checkpoint validity
- no dashboard view presents local optimistic state as committed truth
- no ADE surface shows sessions unrelated to its selected agent or session context
- no generated type drift remains between gateway, OpenAPI, SDK, and dashboard runtime-session consumers

## Required Backend Tests

### List contract

- cursor list returns deterministic ordering when multiple sessions share the same `last_event_at`
- cursor progression does not skip or duplicate tied rows
- list returns `agent_ids` as arrays
- list rejects unsupported legacy query shapes if deprecated behavior is removed

### Detail contract

- `GET /api/sessions/:id` returns 404 for missing session
- detail returns canonical summary data and bookmark count

### Bookmark mutations

- create bookmark fails when `sequence_number` is not present in the session
- delete bookmark cannot remove a bookmark from another session
- create and delete write audit entries using the real `session_id`

### Branch mutations

- branch fails for missing source session
- branch fails for missing checkpoint
- branch fails when zero events would be copied
- successful branch returns a real session summary that can be loaded immediately

## Required SDK Tests

- runtime session wrapper types match generated types exactly
- list, detail, events, bookmarks, and branch requests serialize the canonical fields
- no runtime-session wrapper retains legacy index-based mutation params

## Required Dashboard Tests

- `/sessions` loads additional pages and does not stop at the first backend page
- `/sessions` refreshes on websocket resync through the shared store
- replay bookmark create failure is surfaced and does not leave a fake saved bookmark in UI
- replay delete failure is surfaced and state is rolled back or refreshed
- Agents page only shows sessions containing the selected agent
- Observability session picker and `/sessions` render the same normalized summaries

## Adversarial Checks

- duplicate timestamps across many sessions
- sessions with multiple agents
- empty bookmark list
- missing or malformed event attributes
- branch requests against the first and last event in a session
- session counts above 200 with repeated cursor pagination

## Manual Operator Script

Before calling the work complete:

1. Seed more than 50 runtime sessions.
2. Verify `/sessions` loads beyond the first page.
3. Navigate from an agent to a related session and confirm the agent is present in `agent_ids`.
4. Create a bookmark, reload, and confirm it persists.
5. Force a bookmark failure and confirm the UI does not lie.
6. Branch from a valid checkpoint and load the new session.
7. Attempt an invalid branch and confirm the UI surfaces a hard failure.
8. Trigger websocket resync and confirm the Sessions list refreshes.

## Completion Statement

The subsystem is done only when the dashboard, SDK, and gateway all agree on session identity, pagination, mutation semantics, and cross-surface ownership without route-local compensating logic.
