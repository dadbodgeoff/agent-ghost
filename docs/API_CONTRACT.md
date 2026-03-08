# GHOST ADE — API Backward-Compatibility Contract

> This document defines the rules governing API evolution for the GHOST
> gateway REST and WebSocket interfaces. All contributors and consumers
> must adhere to these guarantees.
>
> Ref: ADE_DESIGN_PLAN Section 5.0.10, tasks.md T-1.3.3

---

## 1. Contract Authority

The public contract has three layers:

1. **Canonical REST contract**
   - Defined by `GET /api/openapi.json`
   - Only routes represented in the OpenAPI document are canonical REST API
     surface
2. **Protocol contracts**
   - WebSocket (`/api/ws`) and transport/discovery routes are real supported
     interfaces, but they are documented as protocol surfaces rather than
     folded into REST/OpenAPI parity
3. **SDK convenience layers**
   - SDK helpers may aggregate or adapt canonical routes
   - These helpers are not separate server-owned domains unless the gateway
     exposes matching endpoints

Current intentional non-REST exclusions from the canonical OpenAPI route set:

- `/api/ws`
- `/api/openapi.json`
- `/a2a`
- `/.well-known/agent.json`

Current convenience-layer decision:

- `ApprovalsAPI` is a compatibility layer over goals/proposals semantics, not a
  distinct approval-domain contract

---

## 2. Versioning Strategy

The API uses **URL-path versioning** with an implicit `v1` prefix.
All current endpoints live under `/api/*` which is equivalent to `/api/v1/*`.

When a breaking change is unavoidable, a new version prefix (`/api/v2/*`)
will be introduced. The previous version will remain available for the
deprecation period defined in Section 5.

---

## 3. Non-Breaking Changes (Always Allowed)

The following changes are considered non-breaking and may be shipped
in any release without a version bump:

| Change | Example |
|---|---|
| Add a new field to a response object | `{"id": "...", "new_field": 42}` |
| Add a new optional query parameter | `?include_deleted=true` |
| Add a new endpoint | `GET /api/workflows` |
| Add a new WebSocket event type | `{"type": "NewEventType", ...}` |
| Add a new enum variant to a string field | `status: "archived"` |
| Widen a numeric range | `score: 0.0-1.0` -> `score: 0.0-2.0` |
| Reduce an error response to a success | `404` -> `200` (with data) |

**Client obligation**: Clients MUST ignore unknown fields and unknown
WebSocket event types. Clients MUST NOT fail on unexpected enum values.

---

## 4. Breaking Changes (Require Version Bump)

The following changes are breaking and require a new API version:

| Change | Why it breaks |
|---|---|
| Remove or rename an existing response field | Clients reading the field will fail |
| Remove or rename an endpoint | Clients calling it will get 404 |
| Change a field's type | `"score": 0.85` -> `"score": "0.85"` |
| Make an optional request field required | Existing requests missing it will fail |
| Change the semantics of an existing field | `level: 0` meant "normal", now means "critical" |
| Remove a WebSocket event type | Clients subscribing to it lose data |
| Change authentication requirements | Unauthenticated -> authenticated |

---

## 5. Deprecation Policy

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

## 6. Error Response Contract

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

## 7. Rate Limiting Headers

Rate limit headers are emitted as follows:

| Header | Description |
|---|---|
| `X-RateLimit-Limit` | Maximum requests per window (success and `429` responses) |
| `X-RateLimit-Reset` | Reset hint in seconds (success and `429` responses) |
| `X-RateLimit-Remaining` | Remaining requests in current window (`429` responses currently emit `0`) |
| `Retry-After` | Seconds to wait (only on `429` responses) |

---

## 8. Request Tracing

All responses include:

| Header | Description |
|---|---|
| `X-Request-ID` | Unique request identifier (UUID v7) for correlation |

Clients may send `X-Request-ID` in requests; the server will use it
if provided, otherwise generate one.

---

## 9. WebSocket Contract

### 9.1 Connection

- Endpoint: `ws://host:port/api/ws`
- Authentication:
  - preferred:
    - `POST /api/ws/tickets` with normal HTTP bearer auth
    - connect with `Sec-WebSocket-Protocol: ghost-ticket.<short_lived_ticket>`
  - deprecated fallback: `Sec-WebSocket-Protocol: ghost-token.<jwt_or_legacy_token>`
  - deprecated fallback: `?token=<jwt_or_legacy_token>`
- Server event wire format:

```json
{
  "seq": 42,
  "timestamp": "2026-03-07T12:00:00Z",
  "event": {
    "type": "ScoreUpdate"
  }
}
```

- Keepalive: server sends an enveloped `Ping` every 30s

### 9.2 Event Types

Server event payloads use an inner `event.type` field:

| Type | Direction | Description |
|---|---|---|
| `Ping` | Server -> Client | Keepalive |
| `ScoreUpdate` | Server -> Client | Convergence score changed |
| `InterventionChange` | Server -> Client | Intervention level changed |
| `AgentStateChange` | Server -> Client | Agent lifecycle state changed |
| `KillSwitchActivation` | Server -> Client | Kill switch activated |
| `ProposalDecision` | Server -> Client | Proposal approved/rejected |
| `SessionEvent` | Server -> Client | New ITP event in a session |
| `Resync` | Server -> Client | Client replay gap detected; perform full refetch |

New event types may be added at any time (non-breaking per Section 3).

### 9.3 Client Messages and Reconnection

Clients may send these top-level JSON messages:

- reconnect replay request: `{"last_seq": N}`
- topic subscribe: `{"type": "Subscribe", "topics": ["agent:<uuid>", "session:<uuid>"]}`
- topic unsubscribe: `{"type": "Unsubscribe", "topics": ["agent:<uuid>"]}`

If the server cannot satisfy a replay request, it sends `Resync`.

### 9.4 Reconnection Strategy

Clients should implement exponential backoff with jitter:

- Initial delay: 1s
- Maximum delay: 30s
- Multiplier: 2x
- Jitter: random 0-1s added to each delay

---

## 10. Pagination Contract

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

## 11. OpenAPI Specification

The canonical API specification is served at:

```
GET /api/openapi.json
```

This endpoint is public (no authentication required) and returns
an OpenAPI 3.1 document generated from handler type annotations.

OpenAPI is the canonical REST contract only. It does not currently model:

- `/api/ws`
- `/.well-known/agent.json`
- `/a2a`
- the schema endpoint itself
