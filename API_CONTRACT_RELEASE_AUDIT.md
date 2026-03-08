# API Contract Release Audit

Status: Superseded on 2026-03-07 by [docs/RELEASE_GAP_VALIDATION_2026-03-07.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/RELEASE_GAP_VALIDATION_2026-03-07.md).

This document is retained as historical audit evidence only. It is not current release truth.

Date: 2026-03-07

Scope:
- gateway mounted route surface
- OpenAPI document truthfulness
- SDK exported contract surface
- WebSocket/public compatibility documentation

Commands used:

```bash
python3 scripts/check_openapi_parity.py
rg -n "ApprovalsAPI|GoalsAPI|GhostWebSocket|generated-types" packages/sdk/src
rg -n '"/api/auth/session"|"/api/oauth/providers"|"/api/studio/sessions"' packages/sdk/src/generated-types.ts
rg -n "Pong|last_seq|Subscribe|Unsubscribe|ghost-token" crates/ghost-gateway/src/api/websocket.rs docs/API_CONTRACT.md
```

## Summary

The next hardening bottleneck is not core runtime execution anymore. It is
public contract truth.

The repo currently presents three conflicting sources of truth:

1. mounted gateway routes in `build_router()`
2. generated OpenAPI at `/api/openapi.json`
3. hand-written SDK exports and compatibility docs

These three surfaces are materially out of sync. That is a release-blocking
problem for OSS/public consumption because consumers cannot tell which API is
canonical, which is convenience-only, and which is undocumented but supported.

## Findings

### P1: OpenAPI is declared canonical, but it only covers a minority of the mounted API

Evidence:
- `python3 scripts/check_openapi_parity.py` reports:
  - mounted routes: 114
  - documented paths: 35
  - undocumented mounted routes: 77
- `build_router()` mounts large undocumented domains including OAuth, studio,
  runtime sessions, marketplace, PC control, channels, traces, and admin
  endpoints.
- `docs/API_CONTRACT.md` explicitly states that `/api/openapi.json` is the
  canonical API specification.

Primary references:
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `docs/API_CONTRACT.md`
- `openapi-parity-audit.md`

Impact:
- generated clients are incomplete by construction
- external users can reasonably conclude real endpoints do not exist
- internal teams can add router paths without any schema consequence

Release judgment:
- release-blocking

### P1: The SDK root export presents a larger public API than the canonical schema describes

Evidence:
- `packages/sdk/src/index.ts` exports both generated OpenAPI types and a much
  larger hand-written client namespace from the same package root.
- The generated OpenAPI types omit major live route domains, but the SDK still
  exports clients for those domains, including OAuth, runtime sessions, studio,
  channels, PC control, and others.
- There is no visible separation in package exports between:
  - schema-backed stable surface
  - convenience wrappers over undocumented endpoints
  - semantic compatibility adapters

Primary references:
- `packages/sdk/src/index.ts`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/package.json`
- `docs/API_CONTRACT.md`

Impact:
- users cannot infer which exports are contract-stable
- any generated-type story is undermined by larger undocumented handwritten APIs
- contract review becomes impossible without source inspection

Release judgment:
- release-blocking

### P1: `ApprovalsAPI` is a semantic alias, not a real approval contract

Evidence:
- `packages/sdk/src/approvals.ts` does not call approval endpoints.
- It fetches `/api/goals` and `/api/goals/{id}`, infers approval type/risk/status
  client-side, and maps proposal data into an invented `Approval` shape.
- `approve()` and `deny()` call goal endpoints directly:
  - `POST /api/goals/{id}/approve`
  - `POST /api/goals/{id}/reject`
- Dashboard approvals UI consumes `client.approvals.*` as if it were a first-class
  domain.

Primary references:
- `packages/sdk/src/approvals.ts`
- `packages/sdk/src/client.ts`
- `dashboard/src/routes/approvals/+page.svelte`
- `crates/ghost-gateway/src/api/goals.rs`

Impact:
- consumers are told there is an approvals API when the platform actually exposes
  proposal/goal lifecycle semantics
- any future divergence between goals and approvals will break callers silently
- the SDK is asserting product semantics that the server contract does not own

Release judgment:
- release-blocking until explicitly demoted or formalized

### P1: Published WebSocket compatibility docs are stale relative to the live protocol

Evidence:
- `docs/API_CONTRACT.md` says:
  - auth is via `?token=...`
  - clients should send `{"type":"Pong"}`
  - messages are simple `{"type": ...}` event payloads
- Live WebSocket server behavior:
  - prefers auth via `Sec-WebSocket-Protocol: ghost-token.<token>`
  - still accepts query param token only as deprecated fallback
  - wraps events in `WsEnvelope { seq, timestamp, event }`
  - supports reconnect replay via `{ "last_seq": N }`
  - supports client `Subscribe` / `Unsubscribe` messages
  - emits `Resync` when replay gaps exist
- The SDK websocket client already implements the richer behavior, which means
  the public docs are behind the real protocol.

Primary references:
- `docs/API_CONTRACT.md`
- `crates/ghost-gateway/src/api/websocket.rs`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`

Impact:
- third-party websocket clients built from docs will be wrong
- reconnect/replay semantics are effectively undocumented
- auth guidance is stale and nudges consumers toward the deprecated path

Release judgment:
- release-blocking for public protocol claims

### P2: The documented rate-limit response contract is stronger than the implementation

Evidence:
- `docs/API_CONTRACT.md` says all responses include:
  - `X-RateLimit-Limit`
  - `X-RateLimit-Remaining`
  - `X-RateLimit-Reset`
- `rate_limit_middleware` sets on successful responses:
  - `x-ratelimit-limit`
  - `x-ratelimit-reset`
- `x-ratelimit-remaining` is only set on `429` responses, not normal successful
  responses.

Primary references:
- `docs/API_CONTRACT.md`
- `crates/ghost-gateway/src/api/rate_limit.rs`

Impact:
- public compatibility doc overstates observable response guarantees
- clients relying on the documented remaining-budget header will fail

Release judgment:
- should fix in same remediation wave as contract cleanup

## What Is Not The Problem

- The dashboard auth boot path appears directionally correct:
  - clears auth only on `401/403`
  - availability failures stay availability failures
- The SDK websocket client is not obviously weaker than the server; the issue is
  documentation and contract framing, not immediate client underimplementation.

References:
- `dashboard/src/routes/+layout.svelte`
- `dashboard/src/lib/auth-boundary.ts`
- `packages/sdk/src/websocket.ts`

## Recommended Closure Order

### Phase 1: Decide the public truth model

Choose exactly one:

1. OpenAPI is the canonical public API
2. SDK is the canonical public API
3. Both exist, but one is explicitly marked convenience/experimental

Current state tries to imply all three and is not defensible.

### Phase 2: Close RG-01

- bring dashboard-used mounted routes into `crates/ghost-gateway/src/api/openapi.rs`
- keep the parity script
- fail CI on new drift

Minimum first-wave route coverage should include:
- `/api/auth/session`
- OAuth routes
- studio routes
- session replay/bookmark routes
- channels
- search
- traces
- profiles
- PC control status

### Phase 3: Close RG-02

Pick one:
- formalize approvals as a real server-owned contract, or
- remove/demote `ApprovalsAPI` and tell consumers to use `GoalsAPI`

My recommendation:
- do not ship `ApprovalsAPI` as a top-level stable domain unless the server
  exposes that semantic contract directly

### Phase 4: Repair public protocol docs

Update `docs/API_CONTRACT.md` to match live websocket behavior:
- envelope shape
- subprotocol auth
- deprecated query auth fallback
- replay and resync
- subscribe / unsubscribe messages

Also either:
- emit `x-ratelimit-remaining` on successful responses, or
- narrow the documented header guarantee

## Exit Criteria

This audit area is closed only when all are true:

- mounted routes are either documented or intentionally excluded by policy
- SDK exports clearly distinguish schema-backed contract from convenience layers
- approvals semantics are either formalized server-side or demoted
- WebSocket docs match live wire behavior
- API compatibility doc stops promising headers or behavior the gateway does not emit
