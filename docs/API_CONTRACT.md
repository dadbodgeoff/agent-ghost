# GHOST ADE — API Backward-Compatibility Contract

> This document defines the rules governing API evolution for the GHOST
> gateway REST and WebSocket interfaces. All contributors and consumers
> must adhere to these guarantees.
>
> Ref: ADE_DESIGN_PLAN §5.0.10, tasks.md T-1.3.3

---

## 1. Versioning Strategy

The API uses **URL-path versioning** with an implicit `v1` prefix.
All current endpoints live under `/api/*` which is equivalent to `/api/v1/*`.

When a breaking change is unavoidable, a new version prefix (`/api/v2/*`)
will be introduced. The previous version will remain available for the
deprecation period defined in §4.

---

## 2. Non-Breaking Changes (Always Allowed)

The following changes are considered non-breaking and may be shipped
in any release without a version bump:

| Change | Example |
|---|---|
| Add a new field to a response object | `{"id": "...", "new_field": 42}` |
| Add a new optional query parameter | `?include_deleted=true` |
| Add a new endpoint | `GET /api/workflows` |
| Add a new WebSocket event type | `{"type": "NewEventType", ...}` |
| Add a new enum variant to a string field | `status: "archived"` |
| Widen a numeric range | `score: 0.0–1.0` → `score: 0.0–2.0` |
| Reduce an error response to a success | `404` → `200` (with data) |

**Client obligation**: Clients MUST ignore unknown fields and unknown
WebSocket event types. Clients MUST NOT fail on unexpected enum values.

---

## 3. Breaking Changes (Require Version Bump)

The following changes are breaking and require a new API version:

| Change | Why it breaks |
|---|---|
| Remove or rename an existing response field | Clients reading the field will fail |
| Remove or rename an endpoint | Clients calling it will get 404 |
| Change a field's type | `"score": 0.85` → `"score": "0.85"` |
| Make an optional request field required | Existing requests missing it will fail |
| Change the semantics of an existing field | `level: 0` meant "normal", now means "critical" |
| Remove a WebSocket event type | Clients subscribing to it lose data |
| Change authentication requirements | Unauthenticated → authenticated |

---

## 4. Deprecation Policy

When an endpoint or field is deprecated:

1. The response includes a `Deprecation` header with the sunset date:
   ```
   Deprecation: Sun, 01 Sep 2026 00:00:00 GMT
   ```

2. Deprecated endpoints return a `301 Moved Permanently` redirect to
   the replacement endpoint for **6 months** after deprecation.

3. After the 6-month window, the deprecated endpoint returns `410 Gone`.

4. Deprecated fields continue to be included in responses for 6 months
   alongside their replacement. A `_deprecated` suffix or documentation
   note signals the transition.

---

## 5. Error Response Contract

All error responses use the standard envelope:

```json
{
  "error": {
    "code": "MACHINE_READABLE_CODE",
    "message": "Human-readable description",
    "details": {}
  }
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `error.code` | string | Yes | Stable machine-readable error code (e.g., `NOT_FOUND`, `RATE_LIMITED`) |
| `error.message` | string | Yes | Human-readable description (may change between releases) |
| `error.details` | object | No | Additional context (validation errors, conflicting fields, etc.) |

Error codes are stable identifiers. New codes may be added; existing
codes will not be removed or have their semantics changed.

---

## 6. Rate Limiting Headers

All responses include rate limit headers:

| Header | Description |
|---|---|
| `X-RateLimit-Limit` | Maximum requests per window |
| `X-RateLimit-Remaining` | Requests remaining in current window |
| `X-RateLimit-Reset` | Unix timestamp when the window resets |
| `Retry-After` | Seconds to wait (only on 429 responses) |

---

## 7. Request Tracing

All responses include:

| Header | Description |
|---|---|
| `X-Request-ID` | Unique request identifier (UUID v7) for correlation |

Clients may send `X-Request-ID` in requests; the server will use it
if provided, otherwise generate one.

---

## 8. WebSocket Contract

### 8.1 Connection

- Endpoint: `ws://host:port/api/ws`
- Authentication: `?token=<jwt_or_legacy_token>` query parameter
- Keepalive: Server sends `{"type": "Ping"}` every 30s; client should respond with `{"type": "Pong"}`

### 8.2 Event Types

All WebSocket messages are JSON with a `type` field:

| Type | Direction | Description |
|---|---|---|
| `Ping` | Server → Client | Keepalive |
| `Pong` | Client → Server | Keepalive response |
| `ScoreUpdate` | Server → Client | Convergence score changed |
| `InterventionChange` | Server → Client | Intervention level changed |
| `AgentStateChange` | Server → Client | Agent lifecycle state changed |
| `KillSwitchActivation` | Server → Client | Kill switch activated |
| `ProposalDecision` | Server → Client | Proposal approved/rejected |
| `SessionEvent` | Server → Client | New ITP event in a session |

New event types may be added at any time (non-breaking per §2).

### 8.3 Reconnection

Clients should implement exponential backoff with jitter:
- Initial delay: 1s
- Maximum delay: 30s
- Multiplier: 2×
- Jitter: random 0–1s added to each delay

---

## 9. Pagination Contract

Paginated endpoints use consistent query parameters and response shape:

**Request**: `?page=1&page_size=50`

**Response**:
```json
{
  "<entity_key>": [...],
  "page": 1,
  "page_size": 50,
  "total": 142
}
```

- `page` is 1-based
- `page_size` maximum is 200
- `total` is the total count before pagination

---

## 10. OpenAPI Specification

The canonical API specification is served at:

```
GET /api/openapi.json
```

This endpoint is public (no authentication required) and returns
an OpenAPI 3.1 document generated from handler type annotations.
