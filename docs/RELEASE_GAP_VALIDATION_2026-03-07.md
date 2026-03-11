# Release Gap Validation And Remediation Plan

Date: 2026-03-07

Scope:
- Validate the current audit findings against the live codebase at current HEAD
- Separate confirmed defects from stale or false-positive findings
- Propose release-grade fixes only for issues that still materially exist

## Executive Summary

The strongest earlier concerns about API contract split are no longer true at current HEAD.
The local parity audit now reports `120` mounted routes, `116` documented paths, and `120`
covered or intentionally excluded routes, with `0` undocumented mounted routes. The
dashboard also no longer uses a fake approvals domain, and `MemoryAPI.graph()` is backed by
both a live route and OpenAPI metadata.

The highest-confidence remaining gaps are narrower:

1. PC control admin APIs have correctness and observability defects, but not the claimed auth gap.
2. Several SDK hardening items are real.
3. Convergence monitor dual-key expiry and stale-state pruning are real.
4. One test-gap claim is real, two are stale.

## Validation Matrix

| Area | Claim | Status | Evidence | Decision |
|---|---|---|---|---|
| PC control | Any valid session can modify global PC control config | False flag | `crates/ghost-gateway/src/bootstrap.rs` mounts PC control writes under admin routes; `crates/ghost-gateway/src/api/rbac.rs` requires `Admin`; `crates/ghost-gateway/src/api/auth.rs` injects claims before RBAC | No security fix required here |
| PC control | Circuit breaker state is hardcoded to `closed` | Confirmed | `crates/ghost-gateway/src/api/pc_control.rs` returns `circuit_breaker_state: "closed"` regardless of runtime state | Fix required before release |
| PC control | Safe zone updates silently drop all but the last zone | Confirmed | `SafeZonesRequest` accepts `zones: Vec<SafeZone>` but `latest_safe_zone()` keeps only `zones.last()` and persists one zone | Fix required before release |
| PC control | No coordinate validation | Partial | The API accepts arbitrary safe-zone rectangles and persists them without validation; negative `x/y` may be valid for multi-monitor layouts, but zero-area and invalid geometry are not screened | Fix required, but target geometry validation not blanket non-negative rules |
| PC control | Action logging is lossy | Partial | Storage keeps `input_json` and `result_json` in `pc_control_actions`, but admin read API collapses output to `target` and `result` summaries | Fix the admin forensic surface, not the storage schema |
| API contract | OpenAPI covers only a minority of mounted routes | Stale | `python3 scripts/check_openapi_parity.py` now reports `0` undocumented mounted routes | Mark old audit as superseded |
| API contract | SDK exports undocumented routes as if schema-backed | Mostly stale | The parity gap is closed and `docs/API_CONTRACT.md` now explicitly distinguishes canonical REST, protocol surfaces, and convenience layers | No blocker; keep contract boundaries explicit |
| API contract | `ApprovalsAPI` is a semantic alias over goals | Stale | No live `ApprovalsAPI` export in `packages/sdk/src/index.ts`; dashboard approvals page uses `client.goals.*` | No fix required |
| API contract | WebSocket docs are stale | Stale | `docs/API_CONTRACT.md` now documents envelope format, subprotocol auth, replay, resync, and subscriptions consistent with `crates/ghost-gateway/src/api/websocket.rs` | No fix required |
| API contract | `MemoryAPI.graph()` is exported with no route | False flag | `crates/ghost-gateway/src/bootstrap.rs` mounts `/api/memory/graph`; handler exists in `crates/ghost-gateway/src/api/memory.rs`; OpenAPI includes it | No fix required |
| Dashboard | Approvals page has a WebSocket decision race that can corrupt state | Not reproduced at current HEAD | `dashboard/src/routes/approvals/+page.svelte` now uses goal proposals directly; WebSocket updates and button actions converge on the same proposal state shape | Treat as UX hardening at most, not a release defect |
| SDK | `Workflow.nodes` and `Workflow.edges` are typed as `unknown` | Confirmed | `packages/sdk/src/workflows.ts` uses `unknown` for both fields | Fix required |
| SDK | No retry logic / backoff for transient HTTP failures | Confirmed | `packages/sdk/src/client.ts` performs a single fetch attempt | Fix required |
| SDK | UUID fallback uses `Math.random()` when `crypto` is unavailable | Confirmed | `packages/sdk/src/client.ts` fills random bytes with `Math.random()` | Fix required |
| SDK | WebSocket token is sent as subprotocol and can leak via proxy logs | Confirmed risk | `packages/sdk/src/websocket.ts` sends `ghost-token.<token>` via `Sec-WebSocket-Protocol`; gateway accepts that format in `crates/ghost-gateway/src/api/websocket.rs` | Fix recommended before broad deployment |
| SDK | Audit export blob fetch has no timeout | Confirmed | `packages/sdk/src/audit.ts` bypasses the shared request helper and does not attach `AbortSignal.timeout(...)` | Fix required |
| Convergence monitor | Dual-key tokens have no expiry | Confirmed | `crates/convergence-monitor/src/intervention/cooldown.rs` stores only a token string; `transport/http_api.rs` claims a 5-minute expiry that is not enforced | Fix required |
| Convergence monitor | Session registry never prunes stale sessions | Confirmed | `crates/convergence-monitor/src/session/registry.rs` tracks sessions indefinitely and exposes no prune path | Fix required |
| Convergence monitor | Rate limiter uses floating-point arithmetic | Confirmed, low severity | `crates/convergence-monitor/src/validation.rs` refills with `elapsed.as_secs_f64()` | Improve, but not release-blocking by itself |
| Duplication | Provider setup logic is duplicated between agent chat and studio paths | Confirmed | `crates/ghost-gateway/src/api/agent_chat.rs` and `crates/ghost-gateway/src/api/studio_sessions.rs` both materialize provider chains/streams | Low-priority cleanup |
| Tests | `pause_blocks_agent_chat_and_studio_execution` missing | Partially confirmed | Exact integration test name is absent; lower-level pause behavior exists in runner/runtime safety tests, but no gateway-level end-to-end test proves both HTTP surfaces reject paused agents | Add the missing integration test |
| Tests | `shell_denied_when_unconfigured` missing | False flag | Present in `crates/ghost-agent-loop/src/tools/builtin/shell.rs` | No fix required |
| Tests | `missing_convergence_state_enters_degraded_or_blocked_mode_per_profile` missing | False flag | Present in `crates/ghost-agent-loop/src/runner.rs` | No fix required |

## Release-Grade Remediation Plan

### P0: PC control admin API correctness and forensic fidelity

This work should be treated as one cohesive hardening pass, not as isolated patches.

Required changes:

1. Split runtime state from persisted config.
   - `GET /api/pc-control/status` should read live circuit-breaker state from a shared runtime object in `AppState`, not synthesize `"closed"` from config.
   - Persisted config and runtime telemetry should be returned as separate sections so operators can tell what is configured versus what is currently active.

2. Fix the safe-zone contract instead of masking it.
   - Either support multiple safe zones end-to-end in `ghost-pc-control`, or change the API to a single `safe_zone` object.
   - Do not accept `Vec<SafeZone>` and silently discard all but the last entry.
   - My recommendation: keep gateway DTOs aligned with the underlying engine now and expose one safe zone until multi-zone support is genuinely implemented.

3. Add explicit geometry validation.
   - Reject zero-width and zero-height regions.
   - Reject integer-overflow-prone rectangles before converting or comparing.
   - Allow negative origins only if the product intentionally supports multi-monitor coordinates; document that behavior.

4. Upgrade the admin audit read model.
   - Keep the existing `pc_control_actions` storage schema.
   - Expand the API response to include `input_json`, `result_json`, `target_app`, `coordinates`, `blocked`, `block_reason`, `agent_id`, and `session_id`.
   - Preserve the compact summary fields for UI display, but do not make them the only forensic view.

Acceptance criteria:
- Runtime breaker state in the API matches live breaker transitions.
- Safe-zone updates are lossless relative to the public contract.
- Invalid geometry is rejected with `400`.
- Operators can reconstruct a PC control action from the admin API without direct DB access.

### P1: SDK hardening

Required changes:

1. Replace `unknown` workflow graph types with explicit DTOs.
   - Introduce `WorkflowNode` and `WorkflowEdge` interfaces in the SDK.
   - Keep an escape hatch for custom node payloads via typed `data` fields, not `unknown` at the top level.

2. Add bounded retry/backoff for idempotent requests.
   - Restrict automatic retries to safe methods by default (`GET`, optionally explicit opt-in for idempotent `PUT`/`POST` with idempotency keys).
   - Use capped exponential backoff with jitter.
   - Never blindly retry auth failures or semantic `4xx` responses.

3. Remove the insecure UUID fallback.
   - In environments without Web Crypto, either require caller-supplied IDs or use a secure platform adapter.
   - Do not silently fall back to `Math.random()` for request or operation identifiers.

4. Move WebSocket auth away from bearer material in the subprotocol.
   - Preferred fix: authenticate the WebSocket via normal HTTP auth/session bootstrap, then send a short-lived, single-use upgrade ticket or perform post-connect auth as the first message.
   - Keep query-string auth deprecated and remove it in the same wave if possible.

5. Put `exportBlob()` on the same timeout policy as the rest of the SDK.
   - Reuse the shared request pipeline or attach an explicit abort signal.

Acceptance criteria:
- Typed workflow access works without `unknown` casts.
- Transient network errors no longer fail immediately for idempotent reads.
- SDK-generated IDs are never derived from `Math.random()`.
- WebSocket bearer material is not exposed in handshake metadata.
- Blob export honors configured timeout behavior.

### P1: Convergence monitor hardening

Required changes:

1. Enforce real dual-key expiry.
   - Store pending token metadata: token hash, issued_at, expires_at, initiator, and intended action.
   - Verify expiry in `confirm_dual_key_change()`.
   - Stop claiming 5-minute expiry in the HTTP layer unless the monitor actually enforces it.

2. Add stale-session and stale-bucket pruning.
   - `SessionRegistry` should prune inactive sessions after a configured idle horizon and remove empty agent indexes.
   - `RateLimiter` should prune idle connection buckets to prevent unbounded map growth.

3. Replace floating-point refill math with integer arithmetic.
   - Track refill timestamps and whole-token accrual deterministically.
   - This is a correctness cleanup more than a production emergency, but it should be done while touching the limiter.

Acceptance criteria:
- Expired dual-key tokens are rejected by implementation, not just by docs.
- Long-running monitor processes do not grow session and rate-limit maps without bound.
- Rate-limit behavior is deterministic under sustained load.

### P2: Test closure

Required changes:

1. Add the missing gateway-level pause test.
   - Exercise both `/api/agent/chat` and `/api/studio/sessions/:id/messages` against a paused agent and assert the surfaced lock status.

2. Regenerate or annotate stale audit artifacts.
   - `API_CONTRACT_RELEASE_AUDIT.md` is now historically useful but operationally stale.
   - Mark it as superseded or regenerate it so resolved blockers do not keep reappearing in review loops.

Acceptance criteria:
- The remediation checklist no longer depends on inference from lower-level tests.
- Future audit reviews do not report already-closed contract defects as live blockers.

### P3: Shared provider construction cleanup

Required changes:

1. Extract provider materialization into one shared module used by both `agent_chat` and `studio_sessions`.
2. Keep one path for provider defaults, key resolution, fallback assembly, and streaming adapter construction.

Why this matters:
- It removes a drift vector in one of the most failure-sensitive parts of the runtime stack.
- It reduces the odds that one surface gets auth, model-default, or provider-order fixes that the other misses.

## Proposed Priority Order

1. PC control correctness and forensic API hardening
2. SDK hardening batch
3. Convergence monitor expiry and pruning
4. Missing gateway-level pause integration test
5. Shared provider extraction
6. Audit-doc cleanup

## Approval Recommendation

Approve remediation work for the confirmed and partial findings only.

Do not spend engineering time "fixing" these items because they are already closed or currently unsupported by evidence:
- PC control per-request auth gap
- OpenAPI minority coverage / major route drift
- `ApprovalsAPI` fake-domain blocker
- stale WebSocket compatibility docs
- missing `MemoryAPI.graph()` route
- missing `shell_denied_when_unconfigured`
- missing `missing_convergence_state_enters_degraded_or_blocked_mode_per_profile`

Approve additional audit-hygiene work to retire stale findings so they stop polluting release judgment.
