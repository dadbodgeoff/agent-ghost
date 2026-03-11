# PC Control Implementation Plan

**Version:** 1.0  
**Date:** March 11, 2026  
**Status:** Normative Execution Plan

## Goal

Implement the target PC Control subsystem with correct live behavior, honest
contracts, and release-grade verification.

## Phase Graph

```text
Phase 0: Authority Lock
  -> accept contract and architecture

Phase 1: Runtime Core
  -> PcControlRuntimeService
  -> live apply path

Phase 2: API and SDK Contract Repair
  -> status model
  -> singular safe-zone semantics
  -> event model

Phase 3: Dashboard Repair
  -> honest UI
  -> runtime subscriptions
  -> telemetry presentation

Phase 4: Execution Wiring
  -> skill/runtime enforcement integration

Phase 5: Verification and Release Gates
  -> unit/integration/live coverage
```

## Phase 0: Authority Lock

Output:

- approved package under `docs/pc-control-cohesion/`

Blockers:

- none

## Phase 1: Runtime Core

## Task 1.1 Create runtime service

Files:

- `crates/ghost-gateway/src/pc_control_runtime.rs` (new)
- `crates/ghost-gateway/src/state.rs`
- `crates/ghost-gateway/src/bootstrap.rs`

Deliver:

- runtime snapshot types
- service constructor on bootstrap
- state storage in `AppState`

## Task 1.2 Add apply pipeline

Files:

- `crates/ghost-gateway/src/pc_control_runtime.rs`
- `crates/ghost-gateway/src/api/pc_control.rs`
- `crates/ghost-gateway/src/config_watcher.rs`

Deliver:

- `apply_from_config_path(source)`
- `apply_from_candidate(source, config)`
- explicit apply result and revision handling

## Task 1.3 Unify config path usage

Files:

- `crates/ghost-gateway/src/config_watcher.rs`
- `crates/ghost-gateway/src/bootstrap.rs`

Deliver:

- watcher uses `state.config_path`
- no independent PC Control path resolution

## Phase 2: API and SDK Contract Repair

## Task 2.1 Replace ambiguous status shape

Files:

- `crates/ghost-gateway/src/api/pc_control.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/pc-control.ts`

Deliver:

- status split into `persisted`, `runtime`, `telemetry`
- remove or explicitly mark any duplicate compatibility fields

## Task 2.2 Repair safe-zone contract

Files:

- `crates/ghost-gateway/src/api/pc_control.rs`
- `packages/sdk/src/pc-control.ts`
- `dashboard/src/routes/pc-control/+page.svelte`
- `crates/ghost-pc-control/src/safety/config.rs`

Deliver:

- singular safe-zone API semantics
- singular SDK helper
- single-zone UI
- compatibility shim only if needed

## Task 2.3 Add explicit runtime-change websocket event

Files:

- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/api/pc_control.rs`
- `crates/ghost-gateway/src/config_watcher.rs`
- `packages/sdk/src/websocket.ts` if needed for typings

Deliver:

- `PcControlRuntimeChange`
- event broadcast from API applies and watcher applies

## Phase 3: Dashboard Repair

## Task 3.1 Remove false multi-zone editor

Files:

- `dashboard/src/routes/pc-control/+page.svelte`

Deliver:

- single-zone draw/edit/clear workflow
- no durable freeform zone labels

## Task 3.2 Subscribe to runtime event

Files:

- `dashboard/src/routes/pc-control/+page.svelte`
- `dashboard/src/lib/stores/websocket.svelte.ts`

Deliver:

- refresh on `PcControlRuntimeChange`
- retain `Resync` behavior

## Task 3.3 Render truthful state

Files:

- `dashboard/src/routes/pc-control/+page.svelte`

Deliver:

- runtime activation state visible
- persisted vs runtime divergence visible when present
- separate cards for throughput, policy budgets, and usage

## Phase 4: Execution Wiring

## Task 4.1 Make disablement live

Files:

- `crates/ghost-pc-control/src/lib.rs`
- PC Control skill constructors/execution sites as needed
- `crates/ghost-gateway/src/skill_catalog/definitions.rs`
- `crates/ghost-gateway/src/skill_catalog/service.rs` only if runtime swap is chosen

Deliver:

- execution path consults current runtime service semantics
- disablement blocks immediately

## Task 4.2 Make validator policy live

Files:

- `crates/ghost-pc-control/src/safety/input_validator.rs`
- PC Control skills under `crates/ghost-pc-control/src/input/`
- window/clipboard skills that enforce policy

Deliver:

- effective allowed apps, safe zone, and blocked hotkeys are runtime-current

## Task 4.3 Clarify budget enforcement

Files:

- `crates/ghost-pc-control/src/lib.rs`
- `crates/ghost-gateway/src/api/pc_control.rs`

Deliver:

- telemetry for configured budgets
- telemetry for observed usage
- no ambiguous single action-budget abstraction

## Phase 5: Verification and Release Gates

## Task 5.1 Unit tests

Add tests for:

- runtime apply success/failure
- config watcher path coherence
- disablement immediate effect
- singular safe-zone API validation
- websocket runtime-change emission

## Task 5.2 Integration tests

Add tests for:

- mutate status then execute PC Control skill -> blocked when disabled
- mutate allowed apps then execute -> new policy observed immediately
- mutate hotkeys then execute -> new block set observed immediately
- watcher-triggered reload updates runtime without restart

## Task 5.3 Live audit

Update live audit to prove:

- dashboard mutation changes effective runtime
- cross-tab refresh occurs from websocket event
- disabled state blocks follow-up live execution
- single safe-zone semantics are rendered honestly

## Definition of Done

The feature is done only if all are true:

1. disabling PC Control blocks follow-up execution without restart
2. status endpoint can distinguish persisted state from effective runtime state
3. safe-zone UI, API, SDK, config, and validator all agree on one contract
4. budget UI reflects real budget semantics
5. websocket update path refreshes the PC Control page correctly
6. live audit proves behavior, not just persistence
