# Active Task Entry Point

## Current Program

The active execution program for this repo is the integrity hardening track:

- [INTEGRITY_HARDENING_PROGRAM.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_HARDENING_PROGRAM.md)
- [INTEGRITY_BUILD_TASKS.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_BUILD_TASKS.md)
- [INTEGRITY_TEST_DOCTRINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_TEST_DOCTRINE.md)
- [REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/REQUEST_IDENTITY_AND_IDEMPOTENCY_DESIGN.md)
- [PROPOSAL_LINEAGE_AND_DECISION_STATE_MACHINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/PROPOSAL_LINEAGE_AND_DECISION_STATE_MACHINE.md)

## How To Execute

Use [INTEGRITY_BUILD_TASKS.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_BUILD_TASKS.md) as the implementation source of truth.

Non-negotiable rules:

- do not satisfy critical-path testing with mocks alone
- do not ship happy-path-only coverage
- do not leave replay, stale-decision, restart, or skew paths untested
- do not keep mutable proposal-resolution semantics as the canonical model
- do not approve by proposal ID alone once the decision contract migration begins

## Release Standard

Release is blocked until the evidence in [INTEGRITY_TEST_DOCTRINE.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/docs/INTEGRITY_TEST_DOCTRINE.md) exists for the touched workstreams.

## Legacy Notes

The older migration hardening plan remains below for reference, but it is not the current execution entrypoint for this integrity program.

# Migration Hardening Tasks

## Objective

Execute the hardening program defined in [design.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/design.md)
and bring the post-migration system to a releaseable state with:

- one truthful contract surface
- one canonical client transport
- one desktop runtime authority
- no silent failure paths
- no fake compatibility abstractions
- adversarial coverage on every critical boundary

## Program Rules

These are blocking rules, not suggestions.

- Do not add any new dashboard gateway transport outside the SDK.
- Do not add any new direct Tauri/plugin imports outside the runtime adapter.
- Do not add any new route-local gateway DTOs when the SDK owns the contract.
- Do not leave a compatibility shim undocumented.
- Do not accept a fallback path that hides drift or data loss.
- Do not ship if a contract mismatch can silently degrade behavior.

## Global Release Gates

Release is blocked until all are true:

- gateway router and exported schema are aligned
- SDK unit and integration tests pass
- dashboard hardening tests pass
- Tauri runtime integration tests pass
- no forbidden raw transport or direct Tauri imports remain
- no fake public contract remains exported as stable
- all critical flows have both happy-path and adversarial coverage

## Enforcement Queries

These must trend to zero outside approved files:

- raw gateway fetch:
  `rg -n "fetch\\(" dashboard/src`
- direct Tauri/plugin bypass:
  `rg -n "@tauri-apps|tauri-pty" dashboard/src`
- fake auth/runtime globals:
  `rg -n "__TAURI__|__GHOST_GATEWAY_PORT__|sessionStorage" dashboard/src`
- unsafe event access:
  `rg -n "as any|msg\\.(level|status|signal_scores)" dashboard/src`

## Workstreams

## W1. Contract Inventory and Freeze

### Goal

Produce a full inventory of:

- gateway REST routes
- gateway websocket events
- SDK domain coverage
- dashboard consumers
- runtime-owned capabilities

### Tasks

- map all mounted routes from `crates/ghost-gateway/src/bootstrap.rs`
- map all SDK modules to actual gateway routes
- map all dashboard routes/stores/components to SDK/runtime usage
- classify every boundary as:
  - `canonical`
  - `transitional`
  - `drifted`
  - `dead`

### Done When

- every dashboard boundary call is accounted for
- every websocket consumer has a declared event owner
- every transitional shim is named and tracked

### Adversarial Tests

- inventory diff test fails when a new route is mounted without schema coverage
- inventory diff test fails when a dashboard consumer subscribes to an unknown
  event type

## W2. WebSocket Contract Unification

### Goal

Establish one canonical realtime transport owned by the SDK.

### Tasks

- define the canonical websocket envelope in the SDK
- support gateway envelope parsing in the SDK client
- support canonical reconnect replay in the SDK client
- standardize websocket auth behavior
- migrate dashboard realtime consumers to the canonical event shape
- remove or reduce dashboard-local websocket transport code

### Must Fix

- flat vs enveloped message drift
- deprecated query-param auth drift
- wrong event-field reads in dashboard stores/components
- missing event coverage for ITP if realtime ITP remains a claimed feature

### Done When

- one websocket transport implementation remains
- all consumers use the same typed event union
- replay semantics are tested and deterministic

### Adversarial Tests

- rejected auth via missing token
- rejected auth via revoked JWT
- reconnect with replay gap inside buffer
- reconnect with replay gap outside buffer triggers `Resync`
- malformed envelope is dropped without corrupting state
- duplicate event delivery does not regress state
- follower-tab leader election does not lose replay cursor

## W3. HTTP Contract Truthfulness

### Goal

Ensure exported HTTP contracts reflect the real gateway surface.

### Tasks

- reconcile `api/openapi.rs` with `build_router`
- add schema coverage for all dashboard-used endpoints
- regenerate SDK types only from truthful schema
- stop exporting generated types if the schema remains incomplete

### Must Fix

- partial OpenAPI that omits mounted routes
- SDK export of stale generated types
- route responses with unstable or underspecified payloads

### Done When

- schema covers the mounted dashboard-used route surface
- CI fails on router/schema drift

### Adversarial Tests

- mounted route missing from schema fails CI
- schema path missing from router fails CI
- response DTO shape mismatch fails SDK integration tests

## W4. Auth and Session Lifecycle Hardening

### Goal

Make auth explicit, server-authoritative, and failure-safe.

### Tasks

- replace shell auth probing through `/api/agents`
- add explicit session/auth validation path
- route logout through server revocation before local clear
- make refresh behavior canonical and testable
- remove auth behavior that maps unrelated failures to login

### Must Fix

- layout redirecting to `/login` on non-auth failures
- local-only logout
- split auth semantics across runtime, dashboard, and service worker

### Done When

- only 401/403 trigger auth reset logic
- logout clears server and runtime state together
- availability failures remain availability failures

### Adversarial Tests

- gateway down at app shell boot
- gateway returns 500 during shell boot
- expired JWT with valid refresh
- revoked refresh token
- logout while gateway unavailable
- stale cached data after logout

## W5. Compatibility Shim Removal

### Goal

Remove or explicitly replace fake abstractions.

### Tasks

- replace `ApprovalsAPI` heuristic mapping with a real contract or rename the UI
  to goals
- remove dead runtime globals and unused injections
- remove transitional auth facade once runtime adapter fully covers ownership
- identify and remove dead command paths such as missing Tauri command hooks

### Must Fix

- approvals semantics inferred from goal payload heuristics
- silent dead desktop features
- exported stable APIs that are really shims

### Done When

- no public shim invents domain semantics
- every transitional surface has an owner, deadline, and removal path

### Adversarial Tests

- approval/goals list with unexpected content shape
- empty content shape
- high-cardinality approval list without N+1 collapse
- missing desktop command should fail loudly in tests, not no-op in UI

## W6. Desktop Runtime Consolidation

### Goal

Make Tauri the actual source of truth for desktop runtime behavior.

### Tasks

- expand runtime adapter to cover notifications
- expand runtime adapter to cover keybinding loading
- expand runtime adapter to cover PTY capability
- remove direct plugin imports from routes/components
- remove dead injected globals if fully obsolete

### Must Fix

- `read_keybindings` call with no Tauri command
- hardcoded PTY shell assumptions
- direct desktop capability calls from dashboard

### Done When

- dashboard desktop behavior is runtime mediated only
- runtime adapter is testable and complete for current desktop features

### Adversarial Tests

- runtime adapter in pure web mode
- runtime adapter in desktop mode with missing capability
- desktop command missing or failing
- PTY unavailable
- notification permission denied

## W7. ITP Contract Repair

### Goal

Make the ITP route truthful and either truly live or explicitly not live.

### Tasks

- define the canonical ITP list payload
- decide whether live ITP websocket events exist
- if live exists, add gateway event, SDK type, and dashboard consumer
- if live does not exist, remove live-stream claims and broken subscription
- ensure privacy-level UI only reflects real data

### Must Fix

- dashboard subscription to non-existent `ItpEvent`
- UI content handling when the API does not provide content

### Done When

- ITP page behavior matches gateway capability exactly

### Adversarial Tests

- empty ITP buffer
- malformed ITP row in DB
- missing optional content fields
- extension disconnected
- live event replay after reconnect if live mode is supported

## W8. Service Worker Safety

### Goal

Prevent offline and cache behavior from undermining auth and contract truth.

### Tasks

- classify authenticated vs unauthenticated cacheable endpoints
- disable or partition authenticated API caching
- clear caches on logout/token rotation
- verify replay queue respects auth and session sequencing

### Must Fix

- shared cache entries surviving auth transitions
- offline fallback with no user-visible staleness signal

### Done When

- service worker cannot surface cross-session data
- offline behavior is explicit and constrained

### Adversarial Tests

- login as user A, cache data, logout, login as user B
- token rotation with cached API responses
- offline replay of non-safety action with stale session seq
- safety write attempt while offline

## W9. Test and CI Hardening

### Goal

Turn drift into failing automation.

### Required Gateway Coverage

- router/schema parity tests
- websocket envelope/auth/replay tests
- ITP endpoint tests
- OAuth endpoint tests
- auth/logout tests
- approvals-or-goals contract tests

### Required SDK Coverage

- request parsing/error handling tests updated to current client logic
- websocket integration tests against live gateway fixture
- domain tests for ITP, OAuth, runtime sessions, auth, approvals/goals
- generated-type parity validation

### Required Dashboard Coverage

- app-shell auth behavior tests
- logout tests
- ITP page tests
- realtime consumer tests for convergence/safety/notifications
- service worker auth/offline tests
- runtime adapter tests

### Required Tauri Coverage

- command registration tests
- lifecycle command tests
- runtime capability tests for keybindings, PTY, notifications

### CI Guardrails

- fail on stale generated types
- fail on router/schema drift
- fail on forbidden direct Tauri imports in dashboard
- fail on forbidden raw gateway fetch in dashboard
- fail on unknown websocket event subscription in dashboard

## Implementation Order

This is the enforced execution order.

1. update this task file and freeze rules
2. fix the highest-risk live drift:
   - websocket contract
   - auth shell/logout
   - broken ITP live path
3. remove fake contracts:
   - approvals shim
   - stale generated export or partial schema
4. consolidate desktop runtime
5. harden service worker
6. complete CI parity gates

No lower-priority cleanup should delay steps 2 and 3.

## Current Critical Implementation Slice

This is the first slice that must land before broader cleanup:

### Slice A

- fix SDK websocket envelope handling
- align dashboard websocket consumers with actual event fields
- stop login redirect on non-auth shell failures
- make logout server-authoritative
- remove broken ITP websocket subscription or add the real event path
- remove dead `read_keybindings` bypass or implement it through runtime

### Slice A Acceptance

- realtime consumers match gateway payloads
- SDK websocket is viable as canonical client
- shell differentiates auth vs availability failures
- logout revokes server state
- ITP page is truthful
- dead desktop command path no longer exists

## Silent Failure Checklist

The implementation is not acceptable if any are still true:

- a dashboard event consumer reads a field the gateway never sends
- a gateway failure redirects the app to login
- logout only clears local state
- a UI feature claims live updates without a backing event contract
- a typed API surface is heuristic but presented as stable
- desktop-only UI calls an unregistered Tauri command
- cached authenticated data can survive identity changes

## Definition of Done

This program is done when:

- the audited findings are either fixed or explicitly retired by design
- every critical boundary has adversarial coverage
- no fake contract remains exported as stable
- no silent degradation path remains for the hardening scope
