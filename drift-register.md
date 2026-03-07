# Drift Register

## Purpose

This file tracks the remaining architectural drift items for the hardening
phase.

Severity:

- `P1`: release-blocking correctness or trust issue
- `P2`: high-priority hardening issue
- `P3`: important but can follow after blocking work

Status:

- `open`
- `in-progress`
- `blocked`
- `closed`

## Register

| ID | Category | Severity | Status | Source Owner | Violating Surface | Impact | Fix Strategy | Required Proof |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| DR-001 | `fake-contract` | `P1` | `closed` | Gateway schema | OpenAPI + generated exports | External users can trust stale or partial contract surface. | Either complete schema parity or demote generated exports from canonical status. | Router/spec parity gate plus export policy decision. |
| DR-002 | `fake-contract` | `P1` | `closed` | Goals/proposals domain | SDK approvals shim and approvals UI | Users see semantically misleading approval data. | Replace with real approval DTOs/endpoints or rename back to goals/proposals. | Semantic contract doc and tests. |
| DR-003 | `duplicate-contract` | `P2` | `closed` | SDK websocket | Dashboard websocket store | Dashboard realtime now reuses the SDK transport path instead of maintaining a second socket client. | Keep dashboard logic limited to leader election, BroadcastChannel fan-out, and view-state routing. | Event inventory and websocket integration coverage. |
| DR-004 | `missing-test` | `P2` | `closed` | Gateway auth | `GET /api/auth/session` | Auth/session route behavior is now proven for missing auth, legacy auth, JWT auth, and JWT revocation on logout. | Keep route-level auth/session tests aligned with future auth mode changes. | Gateway tests for auth/session path. |
| DR-005 | `missing-test` | `P2` | `closed` | Gateway websocket | Replay/auth/resync semantics | Websocket contract drift is now covered by gateway and SDK tests. | Keep the gateway/SDK websocket suites aligned with future event and replay changes. | Gateway + SDK websocket integration tests. |
| DR-006 | `runtime-bypass` | `P2` | `closed` | Runtime adapter | PTY spawn in dashboard terminal | Desktop capabilities are now runtime-mediated and guarded. | Keep dashboard desktop imports confined to runtime-owned files and keep runtime tests current. | Runtime capability matrix and tests. |
| DR-007 | `missing-invariant` | `P2` | `closed` | Service worker policy | Offline/cache/session transitions | Auth/cache/session boundary is now proven by browser tests and CI. | Keep the cache matrix and browser gate current with service-worker policy changes. | Cache policy matrix and auth transition tests. |
| DR-008 | `missing-ci-gate` | `P2` | `closed` | Repo governance | No automated drift prevention | Same classes of drift can be reintroduced in later changes. | Add CI guards for router/schema parity, forbidden desktop imports, unknown event consumers, raw transport bypasses. | CI jobs and failing checks. |
| DR-009 | `dead-transitional-code` | `P3` | `closed` | Runtime/dashboard migration | Transitional auth facade removed | Dashboard auth/session state now flows through runtime token storage, SDK auth methods, and explicit auth-boundary notifications. | Keep auth state invalidation localized to runtime token changes and auth-boundary helpers. | Import cleanup plus dashboard auth/session proof. |
| DR-010 | `missing-test` | `P3` | `closed` | Runtime adapter | Desktop commands and shell resolution | Runtime command behavior is now exercised under test. | Keep runtime command tests in sync with future desktop command additions. | Runtime command tests. |

## Detailed Items

## DR-001: Schema Truth Drift

- Category: `fake-contract`
- Severity: `P1`
- Status: `closed`
- Source owner: gateway schema/export policy
- Evidence:
  - parity checker added at [check_openapi_parity.py](/Users/geoffreyfernald/Documents/New project/agent-ghost/scripts/check_openapi_parity.py)
  - current audit baseline recorded in [openapi-parity-audit.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/openapi-parity-audit.md)
  - schema is served from [openapi.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/openapi.rs)
  - generated types are no longer exported from [index.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/index.ts)
  - mounted route surface is built in [bootstrap.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs#L782)
- Why it matters:
  - typed exports become a false promise to external OSS integrators
- Closure state:
  - mounted routes are fully documented or explicitly excluded by policy
  - parity checker now validates both route/path coverage and `ApiDoc` helper membership
  - public SDK export no longer re-exports generated OpenAPI types
  - CI wiring remains tracked separately under `DR-008`

## DR-002: Approvals Semantic Drift

- Category: `fake-contract`
- Severity: `P1`
- Status: `closed`
- Source owner: gateway goals/proposals contract
- Evidence:
  - canonical SDK surface is [goals.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/goals.ts)
  - dashboard queue now consumes the real proposal contract in [+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/approvals/+page.svelte)
  - the approvals shim has been removed from the public SDK surface in [client.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/client.ts) and [index.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/index.ts)
- Why it matters:
  - the SDK exposes a stable-looking type with unstable semantics
- Closure state:
  - the SDK no longer exports or instantiates `ApprovalsAPI`
  - the dashboard queue uses real goal proposal fields and on-demand detail fetches
  - unsupported semantics such as inferred risk/tool metadata and modified approvals have been removed

## DR-003: Realtime Ownership Drift

- Category: `duplicate-contract`
- Severity: `P2`
- Status: `closed`
- Source owner: SDK websocket
- Evidence:
  - SDK websocket client exists in [websocket.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/websocket.ts)
  - dashboard websocket store now consumes the SDK transport in
    [websocket.svelte.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/stores/websocket.svelte.ts)
  - SDK websocket regression coverage expanded in
    [websocket.test.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/__tests__/websocket.test.ts)
- Why it matters:
  - a second transport implementation can quietly drift from the canonical one
- Closure state:
  - SDK owns websocket auth, replay cursor handling, reconnect, and envelope normalization
  - dashboard store now keeps only leader election, BroadcastChannel fan-out, and event handler routing
  - transport drift between SDK and dashboard is materially reduced to adapter-only behavior

## DR-004: Auth Session Test Gap

- Category: `missing-test`
- Severity: `P2`
- Status: `closed`
- Source owner: gateway auth
- Evidence:
  - new session path is implemented in [auth.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/auth.rs#L783)
  - route-level auth/session coverage exists in
    [auth.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/auth.rs)
- Why it matters:
  - the shell auth contract is now anchored to this endpoint
- Closure state:
  - missing bearer still returns `401`
  - valid legacy bearer returns the authenticated session summary
  - valid JWT bearer returns the authenticated session summary with `mode = jwt`
  - logout revokes JWT access and refresh tokens and the revoked access token is rejected on a later session check

## DR-005: Websocket Regression Test Gap

- Category: `missing-test`
- Severity: `P2`
- Status: `closed`
- Source owner: gateway websocket + SDK websocket
- Evidence:
  - wire contract exists in [websocket.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/websocket.rs)
  - SDK parsing/auth support exists in [websocket.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/websocket.ts)
  - gateway integration coverage exists in
    [websocket_contract_tests.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/tests/websocket_contract_tests.rs)
  - SDK regression coverage exists in
    [websocket.test.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/__tests__/websocket.test.ts)
- Why it matters:
  - future event or replay drift can return silently
- Closure state:
  - gateway tests cover auth, replay, and replay-gap-to-resync behavior
  - SDK tests cover subprotocol auth, reconnect replay state, and malformed
    payload handling

## DR-006: PTY Runtime Ownership Gap

- Category: `runtime-bypass`
- Severity: `P2`
- Status: `closed`
- Source owner: runtime adapter
- Evidence:
  - dashboard terminal now consumes PTY capability only through
    [runtime.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/platform/runtime.ts)
  - the Tauri-specific PTY integration is confined to the runtime owner in
    [tauri.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/platform/tauri.ts)
  - dashboard architecture guard reports zero direct desktop import violations
  - runtime command coverage now exists in
    [desktop.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/src-tauri/src/commands/desktop.rs)
- Why it matters:
  - desktop source-of-truth is still partially bypassed
- Closure state:
  - dashboard PTY consumers are runtime-mediated
  - direct desktop imports remain CI-gated to runtime-owned files
  - runtime command behavior for keybindings, missing capability files, parse
    failure, and shell resolution is exercised under test

## DR-007: Service Worker Session-Safety Gap

- Category: `missing-invariant`
- Severity: `P2`
- Status: `closed`
- Source owner: service worker policy
- Evidence:
  - auth endpoints are protected in [service-worker.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/service-worker.ts#L85)
  - auth-boundary state clearing exists in [service-worker.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/service-worker.ts#L64)
  - cache/queue policy is documented in
    [service-worker-cache-matrix.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/service-worker-cache-matrix.md)
  - browser coverage exists in
    [service-worker-auth.spec.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/tests/service-worker-auth.spec.ts)
  - dashboard CI runs the browser suite in
    [ci.yml](/Users/geoffreyfernald/Documents/New project/agent-ghost/.github/workflows/ci.yml)
- Why it matters:
  - session transition safety is still policy-by-implementation rather than
    policy-by-proof
- Closure state:
  - auth endpoints are proven network-only and uncached
  - bearer-authenticated API requests are proven uncached
  - auth-boundary messages are proven to clear cached API responses
  - auth-boundary messages are proven to clear queued offline actions

## DR-008: Missing CI Drift Gates

- Category: `missing-ci-gate`
- Severity: `P2`
- Status: `closed`
- Source owner: repo governance
- Evidence:
  - CI workflow now runs architecture guards in [ci.yml](/Users/geoffreyfernald/Documents/New project/agent-ghost/.github/workflows/ci.yml)
  - route/schema parity gate remains enforced by [check_openapi_parity.py](/Users/geoffreyfernald/Documents/New project/agent-ghost/scripts/check_openapi_parity.py)
  - dashboard boundary guards are enforced by [check_dashboard_architecture.py](/Users/geoffreyfernald/Documents/New project/agent-ghost/scripts/check_dashboard_architecture.py)
- Why it matters:
  - the same migration drift can re-enter after this phase
- Closure state:
  - CI fails on router/schema parity drift
  - CI fails on forbidden dashboard desktop imports outside runtime-owned paths
  - CI fails on raw dashboard `fetch()` and `new WebSocket()` usage outside approved files
  - CI fails on dashboard websocket subscriptions to unknown gateway event names

## DR-009: Transitional Auth Facade Removal

- Category: `dead-transitional-code`
- Severity: `P3`
- Status: `closed`
- Source owner: runtime token persistence + SDK auth client
- Evidence:
  - canonical auth-boundary helper lives in
    [auth-boundary.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/auth-boundary.ts)
  - dashboard auth/session callers now route through runtime + SDK auth in
    [+layout.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/+layout.svelte),
    [login/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/login/+page.svelte),
    [settings/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/settings/+page.svelte),
    and [studio/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/studio/+page.svelte)
  - dashboard auth/session browser coverage exists in
    [auth-session.spec.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/tests/auth-session.spec.ts)
- Why it matters:
  - leaving a compatibility auth facade in place keeps ownership ambiguous and
    invites future token/session bypasses
- Closure state:
  - dashboard no longer imports a dedicated auth compatibility layer
  - runtime remains the only token persistence owner
  - SDK auth endpoints remain the only dashboard auth/session transport path
  - auth-boundary notifications now provide the explicit cache/session reset hook

## Execution Order

Recommended closure order:

1. DR-001
2. DR-002
3. DR-008
4. DR-005
5. DR-006
6. DR-007
7. DR-003
8. DR-004
9. DR-010
10. DR-009
