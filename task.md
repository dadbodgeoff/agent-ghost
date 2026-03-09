# Studio Pipeline Alignment Task

Status: Closed on March 9, 2026

Objective: bring the live Studio pipeline up to a strict production-grade bar with no contract drift, no duplicate execution semantics, no misleading safety state, no silent fallback behavior, and no UI/runtime disagreement about what the system is doing.

This task plan is based on the live code paths, not on the older architecture docs.

Authoritative spec: `STUDIO_MASTER_REMEDIATION_SPEC.md`

Closeout report: `STUDIO_REMEDIATION_CLOSEOUT.md`

This file is the execution tracker for that spec. If this file conflicts with the master spec on contract ownership, semantics, or exit criteria, the master spec wins.

## Engineering Standard

This work is held to the following bar:

- No undocumented contracts.
- No duplicated critical-path logic without a single owning abstraction.
- No silent compatibility shims in production paths unless they are tested and time-bounded.
- No UI state derived from assumptions that are not emitted by the backend.
- No persistence rows that disagree with the corresponding audit truth.
- No retry, replay, pagination, or streaming behavior that is only “best effort” while presented as deterministic.
- No rollout without adversarial tests and explicit migration/backward-compatibility handling.

## Authoritative Scope

Primary sources of truth for this task:

- `dashboard/src/routes/studio/+page.svelte`
- `dashboard/src/lib/stores/studioChat.svelte.ts`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `packages/sdk/src/sessions.ts`
- `packages/sdk/src/chat.ts`
- `packages/sdk/src/websocket.ts`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/ghost-gateway/src/runtime_safety.rs`
- `crates/ghost-gateway/src/provider_runtime.rs`
- `crates/ghost-agent-loop/src/runner.rs`
- `crates/ghost-agent-loop/src/context/prompt_compiler.rs`

## Confirmed Red Flags

1. Route parity is green while Studio payload contracts still drift.
   - existing parity gate proves mounted-path coverage, not parameter or payload-shape parity.
   - consequence: the project can report "no OpenAPI drift" while Studio contracts are already forked.

2. Studio OpenAPI/generated types are stale and the SDK has forked around them.
   - Studio routes are still modeled with `inline(serde_json::Value)` and missing query params.
   - generated types expose `query?: never` where the SDK manually accepts params.
   - consequence: OpenAPI is not authoritative for Studio.

3. Studio session pagination contract is broken.
   - SDK/frontend send `before`.
   - backend accepts `active_since` and `offset`, not `before`.
   - consequence: load-more semantics are undefined and likely incorrect.

4. Stream recovery contract is inconsistent.
   - SDK makes `after_seq` optional.
   - backend requires it.
   - recovery JSON returns durable event names instead of the live SSE names.
   - consequence: replay is not a single canonical public contract.

5. User message safety status is persisted incorrectly.
   - input inspection computes `warning` / `blocked`.
   - Studio message rows are still persisted as `clean`.
   - consequence: persisted message state disagrees with safety audit state.

6. SSE warning/error contract is drifting.
   - frontend expects structured fields like `warning_type`, `error_type`, `provider_unavailable`, `auth_failed`.
   - backend emits different warning shape and does not consistently emit the structured provider error contract the UI expects.
   - consequence: operator-facing resilience and safety UI is partially false.

7. Studio timestamps are not a safe public contract.
   - backend returns SQLite datetime strings.
   - frontend parses them as though they were RFC3339/ISO timestamps.
   - consequence: timezone and parsing behavior are ambiguous.

8. The SDK WebSocket event union is behind the backend and dashboard.
   - backend emits more event variants than the SDK types represent.
   - consequence: compile-time protection is incomplete for real-time contracts.

9. Studio and generic agent-chat execution paths are duplicated and already diverge.
   - they do not handle output inspection, persistence failure, and recovery semantics the same way.
   - consequence: high-probability future regressions, conflicting fixes, and audit inconsistency.

## Program Goals

### Goal 1: Contract Authority

Every Studio-facing transport contract must have one owner and one machine-verifiable shape.

### Goal 2: Execution Path Unification

There must be one canonical path for “run a live agent turn with streaming, tools, safety, providers, and persistence,” with Studio and generic agent chat as thin adapters, not competing implementations.

### Goal 3: Persistence Truthfulness

Persisted rows, audit rows, transport events, and UI state must agree on safety status, turn state, replay state, and failure state.

### Goal 4: Fail-Closed Runtime Semantics

If persistence, replay, contract parsing, stream recovery, or transport synchronization is uncertain, the system must surface explicit degraded or failed state, not pretend everything is healthy.

## Non-Negotiable Constraints

- No changes that rely on frontend-only fixes for backend truth problems.
- No changes that preserve drift by adding more compatibility branches.
- No transport contract changes without shared typed definitions and tests.
- No merge of Studio and agent-chat logic unless replay and persistence semantics stay explicit and testable.
- No “TODO” placeholders in critical path code after this work lands.

## Workstreams

## Workstream A: Contract Inventory and Authority

Goal: define the canonical transport and persistence contracts for Studio.

Tasks:

1. Enumerate all Studio contracts:
   - session list
   - create/delete session
   - send message blocking
   - send message streaming
   - stream recovery
   - websocket events that Studio consumes
   - client heartbeat/liveness contract

2. For each contract, define:
   - owning module
   - schema owner
   - backward-compatibility story
   - replay/retry semantics
   - failure semantics

3. Generate or centralize shared types for:
   - session list query params
   - SSE warning payloads
   - SSE error payloads
   - stream terminal payloads
   - websocket events consumed by Studio

4. Remove any frontend expectation that is not emitted by the backend.

Acceptance criteria:

- There is one typed definition per Studio transport payload shape.
- UI, SDK, and backend all compile against the same semantic contract.
- Any incompatible contract change is explicit and versioned.

## Workstream B: Session List and Pagination Correctness

Goal: make Studio session list semantics deterministic and truthful.

Tasks:

1. Choose one pagination contract:
   - offset-based only, or
   - cursor/time-based only.

2. Remove the unused alternative from the SDK/UI/backend.

3. If cursor/time-based:
   - implement `before` or equivalent in the backend.
   - define sort order and tie-break behavior.

4. If offset-based:
   - remove `before` from SDK and frontend store.
   - ensure stable ordering across updates.

5. Add explicit duplicate/skip protection in tests.

Acceptance criteria:

- “Load more sessions” never returns duplicate rows for a stable DB snapshot.
- “Load more sessions” never skips rows under the documented ordering contract.
- SDK params exactly match backend query params.

## Workstream C: Safety State Integrity

Goal: make message rows, audits, and UI all reflect the same safety truth.

Tasks:

1. Fix Studio user-message persistence so the message row stores the actual computed safety status.

2. Verify the same correction in:
   - blocking Studio path
   - streaming Studio path
   - any shared helper path

3. Audit assistant-message persistence for the same category of mismatch.

4. Add invariants:
   - if an input scan produced `warning` or `blocked`, persisted message state cannot be `clean`.
   - if a turn is rejected pre-execution, no conflicting assistant state can exist.

5. Add repair/migration handling for already persisted bad rows if needed.

Acceptance criteria:

- Persisted message safety state matches the corresponding scan result.
- Audit query, session reload, and replay all show the same safety outcome.
- Historical inconsistency is either migrated or explicitly detectable.

## Workstream D: SSE Contract Hardening

Goal: make Studio streaming semantically exact and machine-consistent.

Tasks:

1. Define canonical SSE event payloads for:
   - `stream_start`
   - `text_delta`
   - `tool_use`
   - `tool_result`
   - `heartbeat`
   - `warning`
   - `error`
   - `stream_end`

2. Standardize warning payloads.
   - stop emitting ad hoc `code` objects if the client expects typed warning fields.
   - or update client and shared types to consume the real server contract.

3. Standardize provider failure payloads.
   - explicit typed provider failure event or structured `error`.
   - include fallback/terminal semantics explicitly.

4. Ensure every emitted event is parseable by the shared client type layer.

5. Verify event-id semantics.
   - replay ids
   - persisted ids
   - terminal id behavior
   - any synthetic start event behavior

6. Eliminate duplicate or ambiguous stream-start semantics if present.

Acceptance criteria:

- Frontend never depends on fields the backend does not emit.
- Backend never emits fields not described in the shared contract.
- Stream recovery and live streaming produce semantically equivalent event sequences.

## Workstream E: Stream Liveness and Heartbeat Ownership

Goal: separate “works today” from a deliberate stream-liveness contract.

Tasks:

1. Decide whether Studio heartbeat belongs to:
   - dedicated Studio session heartbeat route, or
   - generic runtime session heartbeat route.

2. If Studio-specific:
   - create a dedicated endpoint and storage namespace.
   - remove cross-domain leakage from runtime sessions.

3. If generic:
   - document and enforce that Studio session ids are valid heartbeat keys.
   - validate session existence and ownership explicitly.

4. Make backpressure behavior explicit:
   - stale threshold
   - recovery threshold
   - UI behavior on stale heartbeat

Acceptance criteria:

- Stream liveness behavior is intentional, documented, and validated.
- Heartbeat writes cannot silently succeed for an invalid or wrong session domain.
- Backpressure behavior is observable and testable.

## Workstream F: Canonical Live Turn Runtime

Goal: remove duplicate critical-path orchestration between Studio and agent chat.

Tasks:

1. Identify the common execution pipeline:
   - runtime agent resolution
   - availability checks
   - conversation history load/build
   - runtime safety context build
   - runner construction
   - provider fallback ordering
   - stream event production
   - final persistence
   - websocket side effects

2. Extract a canonical service/module for live turn execution.

3. Keep Studio-specific concerns as adapters only:
   - Studio session persistence model
   - Studio SSE acceptance path
   - Studio websocket topics

4. Keep generic agent-chat-specific concerns as adapters only:
   - API session identity model
   - non-Studio response envelopes

5. Remove duplicated fallback/provider/stream logic from both endpoints.

Acceptance criteria:

- There is one canonical implementation of live turn execution.
- Studio and `agent_chat` differ only at the transport/persistence boundary.
- A provider fallback fix or safety fix lands once, not twice.

## Workstream G: Replay and Recovery Truthfulness

Goal: recovery must be deterministic, not aspirational.

Tasks:

1. Verify that every replayable event is durably persisted before it is treated as replay-safe.

2. Define exact recovery semantics for:
   - dropped SSE after partial text
   - dropped SSE during tool run
   - dropped SSE after final persistence but before terminal event delivery
   - DB degradation during stream-event persistence

3. Ensure the UI’s incomplete/recovered/error states map exactly to backend states.

4. Ensure replay fallback from final assistant message is documented and only used intentionally.

5. Add explicit degraded state when replay safety is lost.

Acceptance criteria:

- Recovery path never pretends to be exact when it is reconstructed.
- Replay-safe vs reconstructed output is explicit in code and behavior.
- UI does not mark a response complete unless backend semantics justify it.

## Workstream H: Observability and Operator Truth

Goal: the operator should see what the system is actually doing, especially under failure.

Tasks:

1. Add structured logs/metrics for:
   - contract parse failures
   - replay fallback usage
   - stream persistence degradation
   - heartbeat staleness
   - provider fallback transitions
   - duplicate-event suppression
   - session pagination anomalies

2. Surface critical Studio degradation states into observability dashboards and audit surfaces.

3. Ensure warning/error event types are visible in both logs and UI.

Acceptance criteria:

- A failing or degraded Studio stream is diagnosable without reading raw source.
- Silent failure classes are removed from the Studio control plane.

## Required Test Matrix

### Contract tests

- SDK query params exactly match accepted backend params.
- SSE event payloads round-trip through shared types.
- websocket payloads consumed by Studio parse exactly as emitted.

### Pagination tests

- stable ordering under repeated list/load-more calls
- duplicate prevention under concurrent new-session creation
- no silent ignore of cursor params

### Safety integrity tests

- input warning persists as warning on message row
- input blocked persists as blocked on message row and matching audit row
- assistant output warning/blocked persists consistently

### Streaming tests

- warning payload shape matches frontend contract
- provider error payload shape matches frontend contract
- tool-use/result events survive replay
- dropped SSE after partial text recovers deterministically
- dropped SSE after terminal persistence returns truthful final state

### Liveness tests

- heartbeat against wrong session domain fails
- stale heartbeat triggers backpressure exactly as specified
- recovered heartbeat resumes normal streaming behavior

### Unification tests

- Studio and `agent_chat` share the same canonical execution service
- provider fallback behavior is identical across both entry points
- safety gate ordering is identical across both entry points

## Rollout Plan

### Phase 0: Containment

- Freeze further Studio contract additions until shared contract ownership exists.
- Fail obviously mismatched cases loudly rather than tolerating drift silently.

### Phase 1: Contract Fixes

- Fix pagination contract.
- Fix safety status persistence.
- Fix SSE warning/error payload contracts.

### Phase 2: Runtime Unification

- Extract canonical live turn execution service.
- Migrate Studio and `agent_chat` onto it behind feature flags if needed.

### Phase 3: Recovery and Liveness Hardening

- Finalize heartbeat ownership.
- Finalize replay/degraded-state semantics.
- add observability hooks and dashboards.

### Phase 4: Cleanup

- Remove obsolete adapters, dead compatibility branches, and duplicate logic.
- Update docs to match code, not the reverse.

## Exit Criteria

This effort is not done until all of the following are true:

- Studio transport contracts are shared, typed, and tested.
- Session pagination semantics are deterministic and correct.
- Message safety state is persisted truthfully.
- Warning/error SSE payloads and UI expectations are aligned.
- Stream heartbeat ownership is explicit and validated.
- Studio and `agent_chat` no longer duplicate critical live-turn orchestration.
- Replay/degraded-state semantics are explicit and operator-visible.
- Documentation reflects the live runtime after the code lands.

## Deliverables

1. Corrected runtime code.
2. Shared Studio contract types and tests.
3. Updated observability for Studio degraded modes.
4. Updated architecture/system map docs.
5. A short remediation report listing:
   - what changed
   - which incompatible contracts changed
   - what was migrated
   - what remains intentionally deferred
