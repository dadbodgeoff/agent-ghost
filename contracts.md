# Contract Inventory

## Purpose

This document is the working source-of-truth inventory for the remaining
hardening phase.

It does not replace implementation truth. It records:

- who owns each boundary
- who is allowed to consume it
- where it is implemented
- where it is currently violated
- what test or CI gate must protect it

Statuses:

- `canonical`: correct owner and path are in place
- `transitional`: usable but still intentionally temporary
- `drifted`: typed or exposed, but not trustworthy enough to treat as stable
- `dead`: present in code or docs, but not actually supported

## Boundary Summary

| Boundary | Owner | Consumers | Status | Notes |
| --- | --- | --- | --- | --- |
| Gateway REST routes | Gateway | SDK, dashboard via SDK | `canonical` | Router is authoritative and route/schema parity is CI-enforced. |
| Gateway OpenAPI export | Gateway | External integrators | `canonical` | Export is parity-checked against mounted routes and no longer masquerades as a public generated SDK contract. |
| SDK HTTP client | SDK | Dashboard | `canonical` | Main transport path is correct after phase-one auth/session fixes. |
| SDK websocket client | SDK | Dashboard, future external clients | `canonical` | SDK now owns websocket auth, replay, reconnect, and envelope normalization for both SDK consumers and the dashboard store. |
| Dashboard websocket store | Dashboard view adapter | Dashboard stores/components | `canonical` | Store is now an adapter layer for leader election, BroadcastChannel fan-out, and view-state routing over the SDK websocket client. |
| Goals/proposals decision queue | Gateway + SDK + dashboard | Dashboard proposal queue | `canonical` | Queue now reflects the real goals contract; the old approvals shim has been removed. |
| ITP snapshot route | Gateway + SDK + dashboard | Dashboard ITP page | `canonical` | Honest snapshot path now; no live contract is claimed. |
| Desktop runtime adapter | Runtime | Dashboard | `canonical` | Desktop capabilities are runtime-mediated, command-tested, and CI-guarded against direct dashboard bypasses. |
| Service worker API cache policy | Dashboard service worker | Browser users | `canonical` | Auth/cache/queue behavior is documented, browser-tested, and CI-enforced for the dashboard boundary. |

## Contract Details

## C1. Gateway REST Surface

- Owner: gateway
- Allowed direct consumers: SDK only
- Primary implementation: [bootstrap.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs)
- Current status: `canonical`
- Invariants:
  - dashboard code does not call mounted routes directly
  - SDK method names map to real gateway endpoints
  - auth semantics are owned by gateway auth middleware and auth endpoints
- Required protection:
  - router-to-schema parity CI
  - route-to-SDK inventory check

## C2. Gateway Schema / OpenAPI Export

- Owner: gateway
- Allowed direct consumers: external integrators, generated SDK tooling
- Primary implementation: [openapi.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/openapi.rs)
- Current export: `/api/openapi.json`
- Current status: `canonical`
- Evidence:
  - mounted routes are defined in [bootstrap.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/bootstrap.rs#L782)
  - parity is enforced by [check_openapi_parity.py](/Users/geoffreyfernald/Documents/New project/agent-ghost/scripts/check_openapi_parity.py)
  - generated OpenAPI types are no longer publicly re-exported from
    [index.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/index.ts)
- Invariants:
  - no schema export should be treated as canonical unless it covers the
    mounted route policy
  - no generated type export should imply support for a route absent from the
    real router
- Required protection:
  - CI route/spec parity gate

## C3. Auth and Session Contract

- Owner: gateway auth + SDK auth client + runtime token persistence
- Allowed direct consumers:
  - gateway: runtime/SDK
  - dashboard: SDK auth methods only
- Primary implementations:
  - [auth.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/auth.rs)
  - [auth.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/auth.ts)
  - [auth-boundary.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/auth-boundary.ts)
  - [ghost-client.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/ghost-client.ts)
  - [+layout.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/+layout.svelte)
- Current status: `canonical`
- Invariants:
  - auth state is checked through explicit auth endpoints
  - only `401` and `403` trigger auth reset
  - logout attempts server revocation before local clear
  - runtime owns token storage
- Closure state:
  - dashboard no longer carries a dedicated auth compatibility facade
  - auth cache invalidation is explicit through `auth-boundary.ts`
  - `GET /api/auth/session` and logout revocation behavior are covered by gateway tests
- Required protection:
  - gateway auth/session endpoint tests
  - dashboard shell auth failure-mode tests

## C4. Gateway WebSocket Wire Contract

- Owner: gateway
- Allowed direct consumers: SDK websocket client, dashboard websocket adapter
- Primary implementation: [websocket.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/websocket.rs)
- Current status: `canonical`
- Canonical wire model:
  - envelope `{ seq, timestamp, event }`
  - auth via `Sec-WebSocket-Protocol`
  - reconnect replay via `{ "last_seq": N }`
- Invariants:
  - dashboard consumers only subscribe to real gateway event types
  - replay and resync semantics are deterministic
  - deprecated query-token auth does not re-enter supported paths
- Closure state:
  - gateway websocket tests cover auth, replay, and replay-gap-to-resync behavior
- Required protection:
  - gateway websocket integration tests

## C5. SDK WebSocket Client

- Owner: SDK
- Allowed direct consumers: dashboard and external TypeScript clients
- Primary implementation: [websocket.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/websocket.ts)
- Current status: `canonical`
- What is correct now:
  - envelope parsing
  - subprotocol auth
  - replay cursor support
  - reconnect lifecycle and malformed payload behavior under test
- Closure state:
  - dashboard consumes the SDK websocket transport instead of owning a second raw socket client
- Required protection:
  - SDK websocket integration tests against gateway test harness

## C6. Dashboard WebSocket Adapter

- Owner: dashboard, but only as a view-state adapter
- Allowed direct consumers: dashboard stores/components only
- Primary implementation: [websocket.svelte.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/stores/websocket.svelte.ts)
- Current status: `canonical`
- Invariants:
  - must not invent new event semantics
  - must not diverge from SDK/gateway wire model
  - must remain thin enough to delete later if SDK takes over fully
- Closure state:
  - adapter is limited to leader election, BroadcastChannel fan-out, and view-state routing over the SDK transport
- Required protection:
  - event subscription inventory
  - guard against unknown event strings in dashboard

## C7. Goals / Proposals Decision Queue

- Owner: gateway goals contract, surfaced through SDK goals client
- Primary implementations:
  - [goals.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/goals.rs)
  - [goals.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/goals.ts)
  - [+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/approvals/+page.svelte)
- Current status: `canonical`
- Invariants:
  - queue data is loaded from `/api/goals`
  - dashboard does not invent approval-specific metadata such as synthetic risk,
    tool names, or mutable argument payloads
  - detail fetches use the real proposal detail endpoint and only on demand
- Compatibility note:
  - the route path remains `/approvals` for now, but the contract and UI copy
    are explicitly proposals/goals rather than a separate approvals domain
- Required protection:
  - SDK contract tests for goals list/get/approve/reject
  - dashboard build coverage for proposal queue route

## C8. ITP Route Contract

- Owner: gateway for data, SDK for transport, dashboard for rendering
- Primary implementations:
  - [itp.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/itp.rs)
  - [itp.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/itp.ts)
  - [+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/itp/+page.svelte)
- Current status: `canonical`
- Invariants:
  - page presents the route as snapshot-only
  - no live ITP websocket contract is claimed
  - no content field is assumed unless gateway adds one explicitly
- Remaining gap:
  - if live mode is desired later, it must be defined as a new first-class
    contract rather than inferred

## C9. Desktop Runtime Adapter

- Owner: runtime
- Allowed direct consumers: dashboard only through runtime interface
- Primary implementations:
  - [runtime.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/platform/runtime.ts)
  - [tauri.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/platform/tauri.ts)
  - [web.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/platform/web.ts)
  - [desktop.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/src-tauri/src/commands/desktop.rs)
- Current status: `canonical`
- What is correct now:
  - notifications mediated
  - keybindings mediated
  - shell resolution mediated
  - PTY capability mediated through runtime
- Required protection:
  - guard against new direct desktop imports outside runtime adapter
  - runtime command tests

## C10. Service Worker Cache Policy

- Owner: dashboard service worker
- Allowed consumers: browser runtime only
- Primary implementation: [service-worker.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/service-worker.ts)
- Current status: `canonical`
- What is correct now:
  - auth endpoints are network-only
  - bearer-authenticated API GETs are not cached
  - auth changes and logout send cache-clear signals
  - queued offline actions are cleared on auth boundaries
- Required protection:
  - auth/cache transition tests
  - endpoint cache policy matrix

## Enforcement Queries

These should trend toward either zero or approved exceptions:

- direct desktop imports outside runtime:
  - `rg -n "@tauri-apps|tauri-pty" dashboard/src`
- raw gateway transport in dashboard:
  - `rg -n "fetch\\(|new WebSocket\\(" dashboard/src`
- potentially misleading auth/cache storage:
  - `rg -n "sessionStorage|localStorage.getItem\\('ghost-gateway-url'\\)" dashboard/src`

## Immediate Follow-Up Artifacts

- [drift-register.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/drift-register.md)
- [release-gates.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/release-gates.md)
