# Studio Master Remediation Spec

Status: March 8, 2026

Purpose: define the authoritative remediation plan to bring the live Studio pipeline, its adjacent SDK and WebSocket contracts, and its shared runtime path up to a strict production engineering bar.

This document is based on the live code, not on older architecture docs. If this spec conflicts with `task.md`, this spec wins.

## Standard

This work is held to the following non-negotiable bar:

- No public contract without one explicit owner.
- No typed client wrapper that forks from the generated contract without an explicit exception record.
- No timestamp, pagination, replay, or streaming semantics that depend on implementation-defined behavior.
- No persistence row that disagrees with the corresponding audit truth.
- No partial replay guarantee presented as full recovery.
- No duplicated critical-path runtime logic with divergent failure behavior.
- No contract gate that only proves route presence while payload drift remains possible.

## Scope

This spec covers:

- Studio REST contracts
- Studio SSE streaming contracts
- Studio stream recovery contracts
- Studio session persistence and safety state
- Studio liveness and replay semantics
- Studio-consumed WebSocket contracts
- the shared live-turn runtime path used by Studio and `agent_chat`
- the contract-generation and parity gates that must prevent recurrence

Primary sources:

- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/api/sessions.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/cortex/cortex-storage/src/queries/studio_chat_queries.rs`
- `crates/cortex/cortex-storage/src/migrations/v037_studio_chat_tables.rs`
- `packages/sdk/src/sessions.ts`
- `packages/sdk/src/chat.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`
- `dashboard/src/lib/stores/studioChat.svelte.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/routes/studio/+page.svelte`
- `scripts/check_openapi_parity.py`

## Confirmed Findings

### F1. The current contract gate is too weak to protect Studio.

The existing parity gate is route-level only. It passed cleanly on March 8, 2026 while Studio still has request/response drift across backend, OpenAPI, generated types, SDK wrappers, and dashboard consumers.

Implication:

- the project currently proves only that paths exist in both places
- it does not prove parameter parity
- it does not prove payload-shape parity
- it does not prove SDK wrappers still match generated types

Evidence:

- `scripts/check_openapi_parity.py`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/sessions.ts`

### F2. Studio HTTP/OpenAPI/generated types are already forked.

Studio OpenAPI helpers use `inline(serde_json::Value)` for most Studio routes and omit real query parameters for session list and stream recovery. Generated types therefore expose `query?: never`, while the hand-written SDK adds its own parameter and result types on top.

Implication:

- OpenAPI is not authoritative for Studio today
- generated types cannot safely drive SDK or dashboard behavior
- SDK wrappers can drift silently from server reality

Evidence:

- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/sessions.ts`

### F3. Studio session pagination is out of contract.

The UI and SDK use `before` for load-more behavior. The backend only accepts `limit`, `offset`, and `active_since`.

Implication:

- load-more behavior is undefined
- cursor-like pagination is being simulated against an offset API
- duplicates, skips, and inconsistent paging under active updates are likely

Evidence:

- `dashboard/src/lib/stores/studioChat.svelte.ts`
- `packages/sdk/src/sessions.ts`
- `crates/ghost-gateway/src/api/studio_sessions.rs`

### F4. Stream recovery contract drift exists even before replay logic.

The SDK declares `after_seq` optional. The backend requires it. The JSON recovery endpoint also returns durable event-log names such as `text_chunk` and `turn_complete`, while the live SSE contract uses `text_delta` and `stream_end`.

Implication:

- the recovery API is not a canonical replay of the live SSE contract
- SDK callers can form requests that the backend rejects
- the client needs separate recovery-specific parsing paths for concepts that should share one semantic contract

Evidence:

- `packages/sdk/src/sessions.ts`
- `dashboard/src/lib/stores/studioChat.svelte.ts`
- `crates/ghost-gateway/src/api/studio_sessions.rs`

### F5. User-message safety persistence is wrong.

Both blocking and streaming Studio paths compute `user_safety_status` from `OutputInspector`, but still persist the user message row with `clean`.

Implication:

- persisted message history disagrees with the audit trail
- moderation history, replay, analytics, and operator trust are compromised
- downstream logic cannot safely treat message rows as truth

Evidence:

- `crates/ghost-gateway/src/api/studio_sessions.rs`

### F6. Studio SSE warning and error payloads drift from what the UI expects.

The frontend expects structured fields such as `warning_type`, `error_type`, `provider_unavailable`, and `auth_failed`. The backend warning path emits `code: "db_persistence_degraded"` and there is no matching backend emission of the provider-specific structured fields the UI reads.

Implication:

- operator-facing resilience status is partially false
- some fallback or auth-failure UI is dead code
- degraded stream behavior is not represented consistently

Evidence:

- `dashboard/src/lib/stores/studioChat.svelte.ts`
- `crates/ghost-gateway/src/api/studio_sessions.rs`

### F7. Studio timestamps are not a safe public contract.

Studio session and message timestamps are stored and returned as SQLite `datetime('now')` strings like `YYYY-MM-DD HH:MM:SS`. The dashboard parses them with `new Date(...)` as though they were web-safe ISO timestamps.

Implication:

- timezone semantics are ambiguous
- parsing behavior is implementation-defined across engines
- Studio time rendering can skew or fail silently

Evidence:

- `crates/cortex/cortex-storage/src/migrations/v037_studio_chat_tables.rs`
- `crates/cortex/cortex-storage/src/queries/studio_chat_queries.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `dashboard/src/routes/studio/+page.svelte`
- `dashboard/src/components/ChatMessage.svelte`

### F8. The WebSocket typed contract is incomplete in the SDK.

The backend `WsEvent` enum and the dashboard store include event kinds such as `AgentConfigChange`, `TraceUpdate`, `BackupComplete`, `WebhookFired`, `SkillChange`, and `A2ATaskUpdate`. The SDK `WsEvent` union does not type those variants.

Implication:

- SDK consumers are under-typed relative to the real gateway contract
- event coverage can regress without compile-time detection
- the dashboard is carrying a separate event taxonomy instead of inheriting the SDK contract

Evidence:

- `crates/ghost-gateway/src/api/websocket.rs`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`

### F9. Studio and `agent_chat` already diverge in failure semantics.

These two routes duplicate the same live-turn orchestration but do not fail the same way.

Examples:

- Studio streaming performs output inspection and persists assistant messages and safety audits.
- `agent_chat` streaming does not run the same output inspection pass before emitting terminal safety.
- Studio streaming degrades with warning events when stream-event persistence fails.
- `agent_chat` streaming marks execution `recovery_required` and fails closed on equivalent persistence failures.

Implication:

- identical runtime bugs will not manifest identically across entry points
- safety, replay, and recovery semantics cannot be trusted to remain aligned
- future fixes will drift unless the execution path is unified

Evidence:

- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`

### F10. Tests are not currently protecting the real Studio contract.

SDK tests encode stale assumptions, including array-shaped session list responses and ISO timestamps that do not match the live Studio backend.

Implication:

- tests can pass while the real contract is wrong
- contract regressions are not gated by the current test suite

Evidence:

- `packages/sdk/src/__tests__/client.test.ts`

## Canonical Ownership Decisions

### D1. Public HTTP request and response authority

Authority:

- Rust request and response structs in gateway handlers
- reflected into `utoipa` OpenAPI using explicit typed schemas

Rules:

- No Studio route may use `inline(serde_json::Value)` once remediation starts.
- `packages/sdk/src/generated-types.ts` must be generated from that OpenAPI and treated as the canonical TS contract surface.
- Hand-written SDK wrappers may add ergonomics, but not invent or rename public contract fields.

### D2. Public SSE authority

Authority:

- a typed Rust Studio SSE schema set
- emitted by `studio_sessions.rs`
- mirrored into TS types used by the SDK and dashboard

Rules:

- live SSE and replay/recovery must share the same semantic event vocabulary
- durable storage event names may remain internal, but the public recovery API must map them back into canonical public event names

### D3. Public WebSocket authority

Authority:

- Rust `WsEvent` and `WsEnvelope`

Rules:

- the SDK event union must match the backend enum exactly
- the dashboard store must consume SDK-exported WS types rather than carrying a separate canonical list
- if `/api/ws` remains excluded from path parity, WS schema parity must still be gated separately

### D4. Persistence authority

Authority:

- storage rows are durable state
- safety audit rows are durable safety evidence

Rules:

- message rows and audit rows must not disagree on computed safety status
- where a message is blocked or degraded, transport responses must not imply a conflicting state

### D5. Timestamp authority

Authority:

- all public API timestamps must be RFC3339 UTC strings

Rules:

- storage may use any internal format
- API responses must normalize before leaving the gateway
- UI code must not parse implementation-defined timestamp formats

### D6. Pagination authority

Authority:

- Studio session list must use one contract only: cursor-based pagination

Rules:

- Studio session listing must not expose both cursor and offset semantics for the same endpoint
- the public list response must include `next_cursor` and `has_more`
- cursor ordering must be stable under concurrent inserts and updates

Recommended cursor sort key:

- `(updated_at desc, id desc)`

### D7. Recovery authority

Authority:

- stream recovery is a public transport contract, not a raw storage dump

Rules:

- `after_seq` is public and optional, defaulting to `0`
- recovery responses must replay canonical event names
- any reconstructed fallback from final DB state must be explicitly marked as reconstructed, not exact replay

### D8. Live-turn runtime authority

Authority:

- one canonical live-turn execution service

Rules:

- Studio and `agent_chat` may differ only in adapter concerns:
  - route-specific request envelopes
  - route-specific persistence surfaces
  - route-specific terminal response envelopes
- provider fallback, tool streaming, safety ordering, persistence-failure semantics, and replay guarantees must be shared

## Contract Ledger

| Surface | Current State | Canonical Owner | Required End State |
| --- | --- | --- | --- |
| `GET /api/studio/sessions` | backend offset/active_since, SDK/UI cursor-like `before` | gateway typed request/response + generated types | cursor-only list contract with typed params and typed response |
| `GET /api/studio/sessions/{id}` | untyped OpenAPI body, manual SDK type | gateway typed schema + generated types | typed session detail contract |
| `POST /api/studio/sessions/{id}/messages` | runtime backed, live execution aware | canonical live-turn service + Studio adapter | same runtime semantics as shared service; typed accepted/completed responses |
| `POST /api/studio/sessions/{id}/messages/stream` | Studio-specific streaming path | canonical live-turn service + Studio SSE adapter | canonical SSE event set with fail-closed replay semantics |
| `GET /api/studio/sessions/{id}/stream/recover` | raw durable event names, required `after_seq` | gateway recovery schema + generated types | canonical replay event contract with optional cursor |
| Studio timestamps | SQLite datetime strings leak to UI | gateway response normalization | RFC3339 UTC only |
| Studio user message safety | message row says `clean`, audit says real status | gateway persistence helpers | message row and audit always agree |
| WebSocket events | backend enum ahead of SDK union | backend enum + generated TS contract | single typed WS taxonomy across backend, SDK, dashboard |
| Stream heartbeat | generic session endpoint accepts any id | explicit liveness contract owner | validated Studio or shared heartbeat namespace with existence checks |
| OpenAPI parity | route-only | route + shape parity gates | failing CI on route or shape drift |

## Required Refactors

### R1. Make OpenAPI authoritative for Studio again.

Required changes:

- replace Studio `inline(serde_json::Value)` schemas with real request and response structs
- add real Studio query parameter definitions to `openapi.rs`
- regenerate `packages/sdk/src/generated-types.ts`
- update Studio SDK wrappers to consume generated types instead of shadow types where possible

### R2. Replace Studio session pagination with one stable cursor contract.

Required changes:

- remove `offset` and `active_since` from the public Studio session-list surface
- add a cursor response model with stable ordering
- update SDK and dashboard to consume `next_cursor`
- add duplicate and skip tests under concurrent session churn

### R3. Normalize Studio recovery into a public replay contract.

Required changes:

- make `after_seq` optional with default `0`
- return canonical public event names from recovery
- explicitly mark reconstructed fallback events when replay is not exact
- add parity tests between live stream events and recovery events

### R4. Fix safety integrity at the persistence boundary.

Required changes:

- persist the real computed `user_safety_status` in both blocking and streaming Studio paths
- audit assistant persistence for the same class of mismatch
- add invariants and tests for message-row and audit-row consistency
- evaluate whether historical bad rows need migration or explicit detection

### R5. Define and enforce one Studio SSE schema.

Required changes:

- type `stream_start`, `text_delta`, `tool_use`, `tool_result`, `heartbeat`, `warning`, `error`, and `stream_end`
- replace ad hoc warning payloads with typed warning payloads
- replace UI-invented provider error parsing with backend-emitted typed errors
- make event-id behavior explicit for live, replayed, and reconstructed events

### R6. Normalize all public Studio timestamps.

Required changes:

- convert all Studio API timestamps to RFC3339 UTC before returning them
- update any tests that currently assume ISO while the backend emits SQLite datetime strings
- add gateway-side contract tests for timestamp format

### R7. Close the WebSocket schema gap.

Required changes:

- export all backend WS event variants into SDK types
- reuse SDK WS types in the dashboard store
- add a parity gate between Rust `WsEvent` and the SDK event union

### R8. Extract a canonical live-turn runtime service.

Required changes:

- factor shared agent resolution, runtime-safety context creation, provider ordering, runner execution, streaming event production, terminalization, and persistence-failure handling into one service
- keep Studio and `agent_chat` as adapters only
- unify output inspection and replay semantics across both entry points

### R9. Make stream liveness intentional.

Required changes:

- either create a Studio-specific heartbeat route, or validate Studio ids explicitly on the shared session heartbeat route
- document stale threshold, pause behavior, and UI consequences
- test invalid ids, stale ids, and resumed liveness

### R10. Replace route-only parity with real contract gates.

Required gates:

- existing route parity gate remains
- OpenAPI parameter and response-shape parity gate added
- generated-types freshness gate added
- SDK wrapper to generated-type parity tests added
- WebSocket enum parity gate added
- Studio SSE live/recovery parity tests added

## Delivery Sequence

### Phase 0. Freeze contract drift.

- no new Studio transport fields
- no new dashboard parsing branches for backend fields that do not exist
- no new manual SDK shadow types for Studio without an explicit exception record

### Phase 1. Restore contract authority.

- type Studio OpenAPI helpers
- regenerate generated types
- introduce WS parity gate
- introduce timestamp-format gate

### Phase 2. Fix integrity bugs.

- fix user-message safety persistence
- normalize timestamps
- align Studio session list contract
- align stream recovery query and event naming

### Phase 3. Harden live streaming semantics.

- finalize typed warning and error payloads
- make replay-safe versus reconstructed output explicit
- harden heartbeat ownership and liveness validation

### Phase 4. Unify runtime execution.

- land canonical live-turn service
- migrate Studio and `agent_chat` onto it
- remove duplicated orchestration

### Phase 5. Remove dead drift.

- delete obsolete manual Studio contract definitions where generated types now cover them
- delete dead UI branches for non-existent SSE fields
- update docs to reflect the remediated system only

## Required Verification

### Contract gates

- `python3 scripts/check_openapi_parity.py --fail-on-drift`
- a new Studio shape-parity gate that fails on missing query params, `unknown` bodies, or stale generated types
- a new WS parity gate that fails when Rust `WsEvent` and SDK `WsEvent` diverge

### SDK tests

- session list response shape is object-based and typed
- session list params match backend contract exactly
- recovery query defaults are correct
- generated types and manual wrappers agree

### Gateway tests

- user warning input persists `warning` on message row and audit row
- user blocked input persists `blocked` on message row and audit row
- Studio timestamps serialize as RFC3339 UTC
- SSE warning and error payloads match the public schema
- recovery emits canonical public event names
- invalid heartbeat ids fail as specified

### End-to-end tests

- Studio load-more produces no duplicates and no skips under concurrent new-session creation
- dropped SSE after partial text yields exact replay or explicit reconstructed state
- persistence degradation yields explicit degraded semantics, not silent optimism
- Studio and `agent_chat` produce the same provider fallback and safety behavior through the shared runtime service

## Exit Criteria

This program is not complete until all of the following are true:

- OpenAPI, generated types, SDK wrappers, and dashboard consumers agree on Studio contracts.
- Route parity and shape parity both gate CI.
- Studio session pagination is cursor-based, deterministic, and tested.
- Stream recovery is a public replay contract rather than a raw storage leak.
- Message rows and safety audits agree on computed safety status.
- Studio timestamps are RFC3339 UTC everywhere.
- SDK and dashboard WebSocket taxonomies match the backend enum.
- Studio and `agent_chat` share one canonical live-turn runtime path.
- degraded, reconstructed, and recovery-required states are explicit in both transport and UI behavior.

## Execution Model

`task.md` is the execution tracker for this spec.

If implementation sequencing needs to change, update `task.md`.
If ownership, contract semantics, or acceptance criteria need to change, update this document first.
