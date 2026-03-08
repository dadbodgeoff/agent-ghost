# Active Task Entry Point

## Mission

Close the validated release gaps from [RELEASE_GAP_VALIDATION_2026-03-07.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/RELEASE_GAP_VALIDATION_2026-03-07.md) and ship a production-grade hardening wave with truthful contracts, deterministic behavior, adversarial coverage, and no silent degradation paths.

## Scope

In scope:
- PC control admin API correctness and forensic fidelity
- SDK hardening
- convergence monitor expiry and pruning
- gateway-level pause coverage
- provider-construction deduplication
- stale audit retirement

Out of scope unless explicitly reopened:
- OpenAPI route parity gap
- fake `ApprovalsAPI` contract
- missing `MemoryAPI.graph()` route
- stale WebSocket wire docs

## Confirmed Workstreams

### W1. PC Control Hardening

Deliver:
- truthful `pc-control/status` response separating persisted config from live runtime state
- circuit breaker state from runtime, not config synthesis
- safe-zone contract with no silent data loss
- geometry validation for persisted safe zones
- forensic action API exposing reconstructable audit detail

Required parameters:
- single source of truth for runtime breaker state
- explicit API contract for one safe zone or true multi-zone support
- zero-width and zero-height rejection
- overflow-safe rectangle validation
- preserve raw action fields: `input_json`, `result_json`, `target_app`, `coordinates`, `blocked`, `block_reason`, `agent_id`, `session_id`

Non-negotiables:
- no silent truncation of client input
- no hardcoded `"closed"` breaker state
- no summary-only audit response for admin forensic use

### W2. SDK Hardening

Deliver:
- typed workflow graph DTOs
- bounded retry/backoff for safe idempotent requests
- secure request/operation ID generation
- WebSocket auth path that does not expose bearer material in handshake metadata
- timeout parity for blob export

Required parameters:
- explicit `WorkflowNode` and `WorkflowEdge` interfaces
- retry policy limited to safe/idempotent operations
- capped exponential backoff with jitter
- no `Math.random()` fallback for security-relevant IDs
- request timeout applied uniformly across standard and blob paths

Non-negotiables:
- do not auto-retry semantic `4xx`
- do not hide auth failure as transport retry noise
- do not keep subprotocol bearer tokens as the long-term production contract

### W3. Convergence Monitor Hardening

Deliver:
- real dual-key expiry enforcement
- stale-session pruning
- stale rate-limit bucket pruning
- deterministic rate-limit refill logic

Required parameters:
- store pending dual-key metadata: token hash, issued time, expiry time, initiator, intended action
- configurable session idle horizon
- pruning of empty agent-session indexes
- integer or equivalent deterministic refill logic

Non-negotiables:
- no doc/implementation mismatch on token expiry
- no unbounded in-memory registry growth
- no indefinite validity window for critical-change confirmation

### W4. Gateway Safety Coverage

Deliver:
- gateway-level tests proving pause blocks both agent chat and studio execution

Required parameters:
- cover `/api/agent/chat`
- cover `/api/studio/sessions/:id/messages`
- assert surfaced lock/error contract, not just lower-level runner behavior

Non-negotiables:
- unit proof alone is insufficient for this workstream

### W5. Provider Construction Consolidation

Deliver:
- one shared provider construction path for agent chat and studio flows

Required parameters:
- shared model defaulting
- shared key resolution
- shared fallback assembly
- shared streaming adapter path

Non-negotiables:
- no drift between chat surfaces on provider setup semantics

### W6. Audit Hygiene

Deliver:
- stale audit artifacts marked superseded or regenerated

Required parameters:
- old blocker docs must not remain active release truth after closure

## Contracts

Authoritative contracts:
- REST: OpenAPI plus live mounted behavior
- WebSocket: live gateway wire behavior and SDK canonical client
- Dashboard: only consume server-owned or explicitly documented convenience contracts
- Admin safety surfaces: must expose enough detail for incident reconstruction

Contract rules:
- no public API accepts data it silently discards
- no SDK export may imply stability that the server contract does not own
- no docs may claim expiry, replay, auth, or state behavior the implementation does not enforce
- runtime state and persisted config must be distinguishable in responses

## Conventions

Implementation conventions:
- prefer one canonical seam per concern
- fail closed on missing safety config
- keep convenience layers explicitly labeled
- preserve backward compatibility unless a shim is already declared non-canonical
- avoid adding new transitional abstractions during remediation

Documentation conventions:
- each changed surface must declare owner, source of truth, and deprecation posture
- stale findings must be marked superseded, not left ambiguous

Testing conventions:
- happy-path only coverage is invalid
- mocks cannot be the only evidence on critical boundaries
- restart, replay, stale-state, and malformed-input paths must be exercised where relevant

## Elections

Leader-election and coordination requirements:
- single-owner realtime behavior must remain deterministic across tabs/clients
- reconnect owner must preserve replay cursor integrity
- leader change must not duplicate or drop state transitions
- retry, replay, and reconnect behavior must be idempotent under election churn

Minimum election-related checks:
- reconnect with valid replay cursor
- reconnect outside replay buffer
- leader/follower handoff without duplicate proposal or safety state transitions
- concurrent approval/status updates do not corrupt final state

## Security

Mandatory security posture:
- admin-only safety mutations remain admin-only
- no bearer material in URLs
- remove bearer material from long-term WebSocket handshake design
- no insecure randomness for request identity
- critical dual-key actions must expire and be auditable

## Adversarial Test Matrix

PC control:
- malformed safe-zone payload
- zero-area safe zone
- overflow-prone coordinates
- conflicting multi-zone payload against single-zone contract
- breaker open, half-open, and cooldown transitions
- action log reconstruction from API output alone

SDK:
- transient network failure with bounded retry
- auth failure with no retry loop
- timeout on blob export
- malformed WebSocket frame
- replay gap inside buffer
- replay gap outside buffer

Convergence monitor:
- expired dual-key token
- reused dual-key token
- stale-session accumulation
- idle rate-limit bucket accumulation
- sustained rate-limit edge behavior under long runtime

Gateway/runtime safety:
- paused agent via agent chat
- paused agent via studio message path
- degraded monitor mode with configured block behavior
- stale convergence state behavior by profile

Election/realtime:
- reconnect during ownership handoff
- duplicate event delivery
- out-of-order replay boundary
- resync after lag

## Production Evidence Gates

Release is blocked until all are true:
- PC control contract is truthful and lossless
- SDK hardening changes are covered by deterministic tests
- convergence dual-key expiry is implemented and tested
- stale registries are pruned under test
- gateway pause behavior is proven at HTTP boundary
- old audit blockers are retired or regenerated

Evidence required:
- source tests
- integration tests
- adversarial tests
- CI guard coverage where drift can recur

## CI Guardrails

CI must fail on:
- router/schema drift
- stale or broken public contract assertions
- forbidden insecure randomness in SDK identity path
- missing timeout coverage for blob export path
- regression in gateway pause enforcement tests

## Execution Order

1. PC control hardening
2. SDK hardening
3. convergence monitor hardening
4. gateway pause integration coverage
5. provider path consolidation
6. stale audit retirement

## Definition Of Done

This task is complete when:
- every confirmed gap in the validation doc is either fixed or explicitly retired
- no false flag remains treated as active release truth
- all touched surfaces have adversarial evidence
- contracts, conventions, and election behavior are explicit and test-backed
- an implementation agent can execute from this file without needing a design rewrite
