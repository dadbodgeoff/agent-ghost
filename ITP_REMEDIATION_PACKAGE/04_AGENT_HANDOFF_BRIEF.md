# Agent Handoff Brief: Build The ITP Capability End To End

Status: March 11, 2026

Use this brief only after reading:

1. `ITP_REMEDIATION_PACKAGE/01_MASTER_SPEC.md`
2. `ITP_REMEDIATION_PACKAGE/02_IMPLEMENTATION_PLAN.md`
3. `ITP_REMEDIATION_PACKAGE/03_VERIFICATION_AND_RELEASE_GATES.md`

This brief is the execution document to hand to an implementation agent.

## Mission

Rebuild the ADE `ITP Events` capability into one cohesive subsystem that is truthful, live, drillable, and contract-safe across:

- extension capture
- gateway ingest
- durable persistence
- websocket transport
- SDK contract
- dashboard route
- session drilldown
- verification gates

## Non-Negotiable Constraints

- Do not ship any misleading field name or label.
- Do not leave the route snapshot-only while describing it as live.
- Do not preserve two divergent extension critical paths.
- Do not add a new shallow detail surface if existing session detail/replay can serve as the durable truth.
- Do not treat tests that only check route existence as sufficient.

## Current-State Reality You Must Assume

- `dashboard/src/routes/itp/+page.svelte` is a snapshot-only route.
- `crates/ghost-gateway/src/api/itp.rs` exposes misleading semantics.
- richer session event detail and replay already exist elsewhere in the ADE.
- websocket infrastructure already exists and supports related session activity.
- the extension code currently has duplicated and divergent JS and TS event pipelines.

## Required End State

When you are done, all of the following must be true:

1. There is one canonical ITP read contract.
2. REST, OpenAPI, SDK, dashboard, and tests agree on that contract.
3. The global ITP route updates live or performs explicitly defined refresh-on-event behavior through websocket wiring.
4. The route handles `Resync` safely.
5. Every relevant row links to durable session-owned detail.
6. Extension-originated event production is unified and its semantics are explicit.
7. Counters and status labels are truthful.
8. Tests prove the system, not just the route presence.

## Recommended Execution Sequence

1. Correct the public contract and field semantics.
2. Unify the extension producer path.
3. Correct gateway ingest and list-query behavior.
4. Add or formalize live websocket delivery for ITP activity.
5. Rebuild the dashboard route around truthful state and session drilldown.
6. Add regression tests and browser-level proof.
7. Run the release gates.

## Likely File Touch Set

- `crates/ghost-gateway/src/api/itp.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/api/agent_chat.rs`
- `crates/ghost-gateway/src/api/studio_sessions.rs`
- `crates/cortex/cortex-storage/src/queries/itp_event_queries.rs`
- `crates/cortex/cortex-storage/src/migrations/`
- `packages/sdk/src/itp.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`
- `dashboard/src/routes/itp/+page.svelte`
- optional new dashboard components or stores
- `extension/src/content/observer.ts`
- `extension/src/background/service-worker.ts`
- `extension/src/background/itp-emitter.ts`
- `extension/src/background/gateway-client.ts`
- tests under gateway, SDK, and dashboard

## Implementation Rules

- Prefer reusing existing session detail and replay routes rather than inventing parallel detail APIs.
- If you introduce new public fields, define them precisely and add tests for their semantics.
- If you keep any local extension fallback queue, expose its meaning clearly and do not confuse it with gateway-persisted truth.
- If websocket transport cannot carry a new first-class ITP event cleanly, formally use `SessionEvent` plus deterministic refetch behavior. Do not leave the route unhooked.
- If schema changes are required, use forward-only migrations and migration tests.

## Minimum Test Proof

You are not done without all of the following:

- gateway API tests for `/api/itp/events`
- SDK contract tests
- websocket live/resync tests
- integration tests for durable event flow
- dashboard browser tests covering:
  - render
  - live update or event-driven refresh
  - drilldown
  - degraded or stale state

## Final Delivery Format

When implementation is complete, provide:

- a short architecture summary
- a concise list of major code changes
- tests run and results
- any migration or contract notes
- residual risks, if any

## Definition Of Done

The ITP subsystem is done only when an operator can open `ITP Events`, trust what each field means, see new activity without manual guesswork, recover from reconnect gaps, and navigate from summary to durable detail without leaving the ADE’s canonical truth path.
