# Sessions Contract Authority

This file defines the authoritative external behavior for the runtime Sessions subsystem.

## Canonical Types

### RuntimeSessionSummary

```json
{
  "session_id": "string",
  "agent_ids": ["string"],
  "started_at": "2026-03-11T14:22:31Z",
  "last_event_at": "2026-03-11T14:25:02Z",
  "event_count": 42,
  "chain_valid": true,
  "cumulative_cost": 0.012345,
  "branched_from": "string|null"
}
```

Rules:

- timestamps are ISO 8601 UTC
- `agent_ids` is always an array
- `cumulative_cost` is server-computed
- `branched_from` is null for an original session

### RuntimeSessionListResponse

```json
{
  "data": ["RuntimeSessionSummary"],
  "next_cursor": "opaque-string-or-null",
  "has_more": true,
  "total_count": 1234
}
```

Rules:

- this is the only list response shape
- legacy page-based payloads are deprecated and removed from the dashboard call path
- `next_cursor` must be opaque and encode both `last_event_at` and `session_id`

### RuntimeSessionDetail

```json
{
  "session": "RuntimeSessionSummary",
  "bookmark_count": 3
}
```

### SessionEvent

```json
{
  "id": "string",
  "event_type": "string",
  "sender": "string|null",
  "timestamp": "2026-03-11T14:22:31Z",
  "sequence_number": 17,
  "privacy_level": "internal",
  "latency_ms": 120,
  "token_count": 42,
  "event_hash": "hex-string",
  "previous_hash": "hex-string",
  "attributes": {}
}
```

### SessionEventsResponse

```json
{
  "session_id": "string",
  "events": ["SessionEvent"],
  "next_after_sequence_number": 17,
  "has_more": true,
  "total": 42,
  "chain_valid": true,
  "cumulative_cost": 0.012345
}
```

Rules:

- event pagination is sequence-based, not array-index-based
- replay uses `sequence_number` as the checkpoint identity

### SessionBookmark

```json
{
  "id": "string",
  "session_id": "string",
  "sequence_number": 17,
  "label": "Checkpoint",
  "created_at": "2026-03-11T14:22:31Z"
}
```

## HTTP Contract

### `GET /api/sessions`

Query:

- `cursor?: string`
- `limit?: number` default `50`, max `200`
- optional future filter keys may be added only through generated types

Behavior:

- sorted by `last_event_at DESC, session_id DESC`
- returns the canonical list response
- no mixed page/cursor modes

### `GET /api/sessions/:id`

Behavior:

- returns 404 if the runtime session does not exist
- returns canonical summary plus bookmark count

### `GET /api/sessions/:id/events`

Query:

- `after_sequence_number?: number`
- `limit?: number` default `100`, max `500`

Behavior:

- events are returned in ascending `sequence_number`
- `chain_valid` and `cumulative_cost` apply to the full session, not only the page slice

### `POST /api/sessions/:id/bookmarks`

Request:

```json
{
  "sequence_number": 17,
  "label": "Checkpoint"
}
```

Behavior:

- validates that session exists
- validates that `sequence_number` exists in that session
- returns `201` with the created bookmark from the server
- writes audit lineage against the real `session_id`

### `DELETE /api/sessions/:id/bookmarks/:bookmark_id`

Behavior:

- delete predicate is `(session_id, bookmark_id)`
- returns 404 when bookmark is absent for that session
- must not delete bookmarks from any other session

### `POST /api/sessions/:id/branch`

Request:

```json
{
  "from_sequence_number": 17
}
```

Behavior:

- validates that source session exists
- validates that the checkpoint exists
- rejects zero-copy branch attempts
- returns `201` with the new canonical `RuntimeSessionSummary`

## Frontend Contract

### Shared store responsibilities

The session store owns:

- session list data
- list cursor state
- load-more state
- error state
- refresh on websocket resync
- invalidation after bookmark and branch mutations

### Route responsibilities

Routes may:

- request store initialization
- request load more
- render loading, empty, and error states

Routes may not:

- bypass the store for list fetches
- invent alternate normalization for agent lists
- optimistically claim bookmark persistence before server confirmation

## Cross-Surface Contract

### Agents page

- only render sessions whose `agent_ids` includes the current agent

### Observability page

- session picker uses the same normalized runtime session summaries as `/sessions`

### Replay page

- slider state maps to event `sequence_number`
- bookmark placement and branch checkpoints use `sequence_number`

## WebSocket Contract

Preferred:

- `SessionChanged { session_id, reason }`

Minimum acceptable:

- all session consumers subscribe to shared `Resync` refresh

If the minimum path is chosen first, the design is still incomplete until the shared store owns the refresh behavior.
