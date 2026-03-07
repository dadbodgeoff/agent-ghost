# Release Gates

## Purpose

This document defines the hard gates for calling the remaining hardening phase
release-ready for public OSS use.

A gate is only satisfied if:

- the condition is true
- the proof exists
- the proof is repeatable

## Gate Summary

| Gate ID | Gate | Type | Owner | Status |
| --- | --- | --- | --- | --- |
| RG-01 | Router/schema truthfulness | CI | gateway | `closed` |
| RG-02 | No fake public domain contract | Manual + tests | gateway + SDK | `closed` |
| RG-03 | Dashboard transport boundary integrity | CI | dashboard | `closed` |
| RG-04 | Desktop runtime boundary integrity | CI + tests | runtime | `closed` |
| RG-05 | Websocket contract regression protection | Tests | gateway + SDK | `closed` |
| RG-06 | Auth/session failure semantics | Tests | gateway + dashboard | `closed` |
| RG-07 | Service-worker auth/session safety | Tests | dashboard | `closed` |
| RG-08 | Runtime command coverage | Tests | runtime | `closed` |

## Detailed Gates

## RG-01: Router / Schema Truthfulness

- Type: CI
- Owner: gateway
- Rationale:
  - exported schema cannot be trusted unless it matches the mounted route policy
- Must be true:
  - mounted dashboard-used routes are represented in the exported schema
  - schema does not claim routes absent from the router
- Evidence:
- parity job comparing `build_router` inventory to exported schema
- current parity result:
  - `120` mounted routes
  - `116` documented paths
  - `0` undocumented mounted routes
  - `0` stale documented paths
- parity job command:
  - `python3 scripts/check_openapi_parity.py --fail-on-drift`
- public SDK no longer re-exports generated OpenAPI types from
  [index.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/index.ts)
- Failure action:
  - block release
  - if parity cannot be achieved in time, remove canonical generated export
  - status:
    - `closed` once the CI parity job is active and green
    status

## RG-02: No Fake Public Domain Contract

- Type: manual decision + tests
- Owner: gateway + SDK
- Rationale:
  - semantically misleading types are worse than missing types
- Must be true:
  - `ApprovalsAPI` is either backed by a real approval contract or clearly
    demoted to goals/proposals semantics
- Evidence:
  - public SDK surface now exposes goals/proposals rather than approvals
  - dashboard proposal queue is wired to [goals.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/goals.ts)
  - regression coverage in [client.test.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/__tests__/client.test.ts)
- Failure action:
  - block release

## RG-03: Dashboard Transport Boundary Integrity

- Type: CI
- Owner: dashboard
- Rationale:
  - dashboard drift reappears when raw transport or duplicate transport logic
    returns
- Must be true:
  - no raw gateway HTTP usage outside approved infrastructure files
  - no unsupported websocket event subscriptions
- Suggested checks:
  - `rg -n "fetch\\(" dashboard/src`
  - `rg -n "new WebSocket\\(" dashboard/src`
  - `rg -n "ItpEvent|UnknownEventName"` against approved event inventory
- Failure action:
  - fail CI
  - status:
    - `closed` for raw-transport and unknown-event drift via the dashboard architecture guard

## RG-04: Desktop Runtime Boundary Integrity

- Type: CI + tests
- Owner: runtime
- Rationale:
  - desktop-specific behavior must not silently spread through the dashboard
- Must be true:
  - no direct `@tauri-apps/*` or `tauri-pty` imports outside approved runtime
    files or documented exceptions
  - every dashboard desktop capability has a runtime owner
- Current approved dashboard exception list:
  - [tauri.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/platform/tauri.ts)
- Current audit result:
  - `rg -n "@tauri-apps|tauri-pty" dashboard/src`
  - only the centralized runtime owner file remains
- Evidence:
  - dashboard architecture guard in
    [check_dashboard_architecture.py](/Users/geoffreyfernald/Documents/New project/agent-ghost/scripts/check_dashboard_architecture.py)
    reports zero runtime boundary violations
- Suggested checks:
  - `rg -n "@tauri-apps|tauri-pty" dashboard/src`
- Failure action:
  - fail CI unless the hit is in an approved exception list

## RG-05: Websocket Contract Regression Protection

- Type: tests
- Owner: gateway + SDK
- Rationale:
  - envelope/auth/replay drift is high-risk and easy to reintroduce
- Must be true:
  - auth path is tested
  - replay path is tested
  - replay-gap-to-resync path is tested
  - malformed envelope behavior is tested
- Evidence:
  - gateway integration tests in
    [websocket_contract_tests.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/tests/websocket_contract_tests.rs)
    cover auth, replay, and replay-gap-to-resync behavior
  - SDK tests in
    [websocket.test.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/packages/sdk/src/__tests__/websocket.test.ts)
    cover protocol selection, reconnect replay state, and malformed-message handling
- Failure action:
  - block release

## RG-06: Auth / Session Failure Semantics

- Type: tests
- Owner: gateway + dashboard
- Rationale:
  - users must not be logged out because the service is down
- Must be true:
  - only `401/403` trigger auth reset
  - `500` and network failures remain availability failures
  - logout attempts server revocation and local clear coherently
- Evidence:
  - gateway auth/router tests in
    [auth.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/crates/ghost-gateway/src/api/auth.rs)
    prove `401` on missing bearer when auth is enabled, authenticated session summaries for both legacy and JWT auth, JWT revocation on logout, and logout cookie clearing without requiring prior auth
  - dashboard shell tests in
    [auth-session.spec.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/tests/auth-session.spec.ts)
    cover boot `401`, `403`, `500`, network failure, and logout success/failure semantics
  - existing redirect coverage in
    [mobile.spec.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/tests/mobile.spec.ts)
    now explicitly forces `/api/auth/session` `401` instead of relying on a false-positive all-`200` mock
- Failure action:
  - block release

## RG-07: Service Worker Auth / Session Safety

- Type: tests
- Owner: dashboard
- Rationale:
  - offline support must not undermine auth truth
- Must be true:
  - auth endpoints are never cached
  - cached authenticated data cannot bleed across logout or session transitions
  - queued offline actions respect session sequencing rules
- Evidence:
  - service-worker browser tests in
    [service-worker-auth.spec.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/tests/service-worker-auth.spec.ts)
    prove auth endpoints are not cached, bearer-authenticated API requests are
    not cached, auth-boundary messages clear API cache entries, and
    auth-boundary messages clear queued offline actions
  - auth-boundary notifications are centralized in
    [auth-boundary.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/lib/auth-boundary.ts)
    and emitted on both token set and token clear from
    [login/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/login/+page.svelte),
    [settings/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/settings/+page.svelte),
    [studio/+page.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/studio/+page.svelte),
    and [+layout.svelte](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/routes/+layout.svelte)
  - worker auth-boundary handling clears both API cache entries and pending actions in
    [service-worker.ts](/Users/geoffreyfernald/Documents/New project/agent-ghost/dashboard/src/service-worker.ts)
  - cache policy matrix in
    [service-worker-cache-matrix.md](/Users/geoffreyfernald/Documents/New project/agent-ghost/service-worker-cache-matrix.md)
    documents cache, queue, and auth-boundary behavior
  - dashboard CI runs the browser suite in
    [ci.yml](/Users/geoffreyfernald/Documents/New project/agent-ghost/.github/workflows/ci.yml)
- Failure action:
  - block release

## RG-08: Runtime Command Coverage

- Type: tests
- Owner: runtime
- Rationale:
  - desktop behavior should fail in tests, not on user machines
- Must be true:
  - keybindings load path is tested
  - shell resolution is tested
  - capability failure behavior is tested
- Evidence:
  - runtime command tests for
    [desktop.rs](/Users/geoffreyfernald/Documents/New project/agent-ghost/src-tauri/src/commands/desktop.rs)
    cover missing keybindings, valid keybindings load, malformed keybindings
    failure, invalid keybindings path read failure, env-driven shell resolution,
    blank-shell fallback, and platform fallback shell behavior
- Failure action:
  - block broad OSS desktop release

## Suggested CI Checks

These should be implemented to back the gates:

- router/schema parity job
- `python3 scripts/check_openapi_parity.py --fail-on-drift`
- `pnpm --dir packages/sdk test`
- `pnpm --dir dashboard build`
- `cargo test -p ghost-gateway --test test_critical_path --quiet`
- desktop crate `cargo check --manifest-path src-tauri/Cargo.toml --quiet`
- forbidden dashboard direct desktop import check
- forbidden dashboard raw transport check
- websocket event inventory check

## Gate Review Order

Review in this order:

1. RG-01
2. RG-02
3. RG-04
4. RG-05
5. RG-06
6. RG-07
7. RG-03
8. RG-08

The order is intentional. Contract truth comes before consumer cleanup.
