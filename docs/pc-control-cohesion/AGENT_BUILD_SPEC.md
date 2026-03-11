# PC Control Agent Build Spec

**Version:** 1.0  
**Date:** March 11, 2026  
**Status:** Implementation Handoff Specification

## Mission

Build the PC Control subsystem into a coherent ADE feature with truthful
operator controls, live runtime enforcement, aligned contracts, and
release-grade verification.

This is not a UI polish task. It is a runtime-correctness and safety-remediation
project.

## Required Reading

Before changing code, read:

1. `docs/pc-control-cohesion/PACKAGE_INDEX.md`
2. `docs/pc-control-cohesion/CURRENT_STATE_AUDIT.md`
3. `docs/pc-control-cohesion/TARGET_ARCHITECTURE.md`
4. `docs/pc-control-cohesion/CONTRACT_SPEC.md`
5. `docs/pc-control-cohesion/IMPLEMENTATION_PLAN.md`
6. `docs/pc-control-cohesion/VERIFICATION_PLAN.md`

## Non-Negotiable Outcome

After implementation:

- toggling PC Control off must block follow-up PC Control execution without
  restart
- dashboard state must reflect effective runtime truth, not only persisted YAML
- safe-zone behavior must be singular and honest end to end
- budget reporting must distinguish throughput from policy budgets
- websocket-driven refresh must work for PC Control config/runtime changes

## Design Decisions Already Locked

1. Introduce `PcControlRuntimeService` as runtime owner.
2. Use `state.config_path` as the authoritative config path.
3. Treat singular safe-zone semantics as canonical in this remediation.
4. Add a dedicated runtime-change websocket event.
5. Prefer live runtime policy rebinding over restart semantics.

Do not reopen these decisions during implementation unless a hard technical
constraint makes them impossible. If so, document the blocker explicitly before
changing course.

## Implementation Order

## Step 1: Runtime service

Create a runtime service that:

- reads current PC Control config
- validates and normalizes it
- applies it atomically
- exposes current runtime snapshot
- records revision and apply metadata

Suggested file:

- `crates/ghost-gateway/src/pc_control_runtime.rs`

Required integrations:

- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/bootstrap.rs`

## Step 2: API mutations must apply runtime, not just write YAML

Update:

- `crates/ghost-gateway/src/api/pc_control.rs`

Every mutation route must:

1. validate request
2. persist config
3. call runtime apply
4. emit runtime-change event
5. return current status from runtime snapshot plus persisted state

## Step 3: Config watcher must use the same runtime apply path

Update:

- `crates/ghost-gateway/src/config_watcher.rs`

Requirements:

- watch `state.config_path`
- apply through the runtime service
- emit the same event contract as API mutations

## Step 4: Repair status contract

Update:

- `crates/ghost-gateway/src/api/pc_control.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/pc-control.ts`

Required response shape:

- `persisted`
- `runtime`
- `telemetry`

Do not leave ambiguous top-level convenience fields unless explicitly marked and
tested as compatibility.

## Step 5: Repair safe-zone contract

Update:

- `dashboard/src/routes/pc-control/+page.svelte`
- `packages/sdk/src/pc-control.ts`
- `crates/ghost-gateway/src/api/pc_control.rs`

Requirements:

- single safe-zone editor
- clear action
- no multiple zone list
- no durable custom label expectation

## Step 6: Add dedicated websocket event

Update:

- `crates/ghost-gateway/src/api/websocket.rs`
- `dashboard/src/lib/stores/websocket.svelte.ts`
- `dashboard/src/routes/pc-control/+page.svelte`

Requirements:

- new `PcControlRuntimeChange` event
- page refetch on event
- preserve `Resync` fallback

## Step 7: Make enforcement live

Update:

- PC Control skill/runtime integration

Requirements:

- execution must consult live enabled state
- validator-relevant policy must be live
- circuit-breaker state must remain coherent with runtime service ownership

Preferred implementation:

- give PC Control skills a runtime policy handle or resolver

Avoid:

- partial runtime swaps
- restart-only semantics
- hidden stale bootstrap dependencies

## Step 8: Clarify budget telemetry

Update:

- API status model
- dashboard presentation

Requirements:

- separate throughput/rate section
- separate configured budget section
- separate observed usage section

## Step 9: Verification

Implement all verification required by:

- `docs/pc-control-cohesion/VERIFICATION_PLAN.md`

Minimum proof required before completion:

1. integration test proves disablement blocks live execution
2. integration test proves allowlist/hotkey/safe-zone updates are live
3. dashboard test proves websocket refresh path
4. live audit proves runtime truth, not just persistence

## File Touch Expectations

Expect to modify at minimum:

- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/config_watcher.rs`
- `crates/ghost-gateway/src/api/pc_control.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/pc-control.ts`
- `dashboard/src/routes/pc-control/+page.svelte`
- relevant PC Control runtime/skill files under `crates/ghost-pc-control/src/`

Likely new file:

- `crates/ghost-gateway/src/pc_control_runtime.rs`

## Acceptance Criteria

Do not mark complete unless all are true:

1. Operator disables PC Control in dashboard.
2. Status endpoint reports runtime disabled.
3. Runtime-change websocket event is emitted.
4. Second client refreshes automatically.
5. Representative PC Control execution attempt is blocked without restart.
6. Action/audit evidence is recorded correctly.
7. UI exposes only singular safe-zone behavior.
8. Budget displays are semantically accurate.

## Failure Handling

If runtime apply fails:

- preserve last valid runtime snapshot
- surface explicit failure state in status
- emit operator-visible warning path
- do not silently claim success

## Definition of Success

Success is not:

- persisted config changed
- page rerendered
- tests were updated to match stale behavior

Success is:

- runtime behavior, contracts, and operator surface all say the same thing
- the subsystem behaves correctly under live mutation without restart
