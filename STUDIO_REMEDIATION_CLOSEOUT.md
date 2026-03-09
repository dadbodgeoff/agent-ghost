# Studio Remediation Closeout

Date: March 9, 2026

Status: Closed

Scope:
- Studio session-backed REST contracts
- Studio SSE streaming and recovery contracts
- Studio-consumed WebSocket contract parity
- Studio liveness and persistence truthfulness
- the shared live-turn runtime path used by Studio and `agent_chat`
- adjacent public contract hardening required to keep Studio on one authority model

## Outcome

The Studio remediation program defined in [task.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/task.md) and [STUDIO_MASTER_REMEDIATION_SPEC.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/STUDIO_MASTER_REMEDIATION_SPEC.md) is complete.

The implementation now meets the written exit criteria:
- Studio transport contracts are shared, typed, and tested.
- Studio session pagination is cursor-based, deterministic, and tested.
- Message safety state persists truthfully.
- warning/error payloads and UI expectations are aligned.
- stream heartbeat ownership is explicit and validated.
- Studio and `agent_chat` share canonical live-turn runtime ownership.
- degraded, reconstructed, and recovery-required states are explicit in transport and UI behavior.
- architecture and remediation docs describe the live runtime rather than the older V2 assumptions.

## What Changed

### 1. Contract authority was restored

- Studio REST routes now publish typed OpenAPI request and response schemas instead of broad `inline(serde_json::Value)`.
- `packages/sdk/src/generated-types.ts` was regenerated from the live OpenAPI and the Studio SDK wrappers were moved onto generated contract types for blocking and recovery paths.
- shape-parity, generated-types freshness, and WebSocket parity checks were added so CI now fails on the classes of drift that previously passed.
- the broader public helper surface was brought into the same authority model so Studio no longer sits beside large neighboring `unknown` or raw-JSON holes.

### 2. Studio session and replay semantics were corrected

- session listing is now cursor-based with `cursor`, `next_cursor`, and `has_more`.
- recovery now accepts optional `after_seq`, emits canonical public event names, and marks reconstructed fallback output explicitly with `reconstructed: true`.
- Studio public timestamps are normalized to RFC3339 UTC at the gateway boundary.

### 3. Runtime and persistence truth were aligned

- Studio now persists the real computed `user_safety_status` in both blocking and streaming paths.
- persistence degradation is surfaced as a typed warning (`warning_type: "persistence_degraded"`) instead of an unowned ad hoc client assumption.
- provider/runtime terminal failures now use structured error typing (`error_type`) and fail-closed semantics.
- unknown or deleted session ids no longer silently create heartbeat state.

### 4. Live-turn execution was unified

- shared runtime preparation and blocking execution now live under `runtime_execution.rs`.
- shared streaming provider iteration, timeout, fallback eligibility, and terminal error behavior now live under `stream_runtime.rs`.
- Studio and `agent_chat` now consume the same runtime authority for the critical live-turn path instead of maintaining competing orchestration logic.

### 5. Operator-visible degraded states are now truthful

- replayed synthetic terminal errors and fallback reconstruction are explicitly marked as reconstructed.
- recovery-required and degraded states are carried through transport and dashboard behavior rather than being inferred client-side.
- the dashboard Studio store now parses and displays the same warning/error/recovery contract the backend emits.

## Incompatible Contract Changes

The following public contract changes are intentional and breaking for any caller that depended on the older drifted behavior:

1. `GET /api/studio/sessions`
   - old drifted behavior: SDK/UI used `before` against a backend offset-style API.
   - new contract: cursor-based pagination with `cursor`, `next_cursor`, and `has_more`.

2. `GET /api/studio/sessions/{id}/stream/recover`
   - old drifted behavior: recovery leaked durable event names and effectively required `after_seq`.
   - new contract: `after_seq` is optional and recovery returns canonical public event names such as `text_delta` and `stream_end`.

3. Studio timestamps
   - old behavior: SQLite datetime strings leaked into the public API.
   - new contract: Studio timestamps serialize as RFC3339 UTC.

4. Studio SSE warning and error payloads
   - old drifted behavior: UI expected structured fields the backend did not consistently emit.
   - new contract: warning and terminal error payloads carry typed fields such as `warning_type`, `error_type`, and `reconstructed` where applicable.

5. Generated SDK types
   - old behavior: several Studio and adjacent routes code-generated as `unknown` or with `query?: never`, forcing manual shadow contracts.
   - new contract: the relevant OpenAPI-backed operations now generate typed request and response shapes.

## What Was Migrated

### Runtime migration

- Studio and `agent_chat` were migrated onto shared runtime authority for blocking and streaming execution.

### Contract migration

- Studio blocking, recovery, and neighboring public route families were migrated from manual or raw-JSON contract descriptions to typed OpenAPI-backed schemas and regenerated SDK types.

### Client migration

- the dashboard Studio and WebSocket consumers were migrated to the remediated typed transport model.

### Data migration

- no dedicated Studio data-rewrite migration was introduced as part of this remediation.
- the remediation fixed forward behavior at the gateway boundary and made replay/degraded-state truth explicit.

## What Remains Intentionally Deferred

1. Studio SSE generation is still not derived from OpenAPI.
   - explicit exception record remains in [packages/sdk/src/chat.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/chat.ts).
   - the hand-maintained `StreamEvent` union is still allowed until there is an authoritative generated SSE schema source.

2. Non-REST protocol surfaces remain intentionally outside OpenAPI parity.
   - `/api/ws`
   - `/.well-known/agent.json`
   - `/a2a`
   - `/api/compatibility`
   - `/api/openapi.json`

3. `POST /api/studio/run` remains Studio-adjacent rather than part of the session-backed Studio turn pipeline.
   - it is now typed and gated, but it is not the canonical session-backed Studio execution path.

## Verification Run During Remediation

The remediation was closed only after the following categories of verification passed during the implementation sequence:

- gateway tests: `cargo test -p ghost-gateway`
- targeted gateway suites covering critical path, operation journal, and websocket/recovery behavior
- SDK tests: `pnpm --dir packages/sdk exec vitest run src/__tests__/client.test.ts src/__tests__/websocket.test.ts`
- SDK type checks and build: `pnpm --dir packages/sdk typecheck` and `pnpm --dir packages/sdk build`
- dashboard type and architecture checks: `pnpm --dir dashboard check` and `pnpm audit:architecture:strict`
- contract gates:
  - `python3 scripts/check_openapi_parity.py --fail-on-drift`
  - `python3 scripts/check_generated_types_freshness.py`
  - `python3 scripts/check_ws_contract_parity.py`
  - `python3 scripts/check_studio_shape_parity.py`

## Closeout Judgment

This program is closed.

The remaining exceptions are explicit, documented, and intentional rather than hidden drift. Future Studio changes should be evaluated against the gates and ownership rules in [STUDIO_MASTER_REMEDIATION_SPEC.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/STUDIO_MASTER_REMEDIATION_SPEC.md).
