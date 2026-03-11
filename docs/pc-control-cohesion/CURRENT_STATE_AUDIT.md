# PC Control Current State Audit

**Date:** March 11, 2026  
**Status:** Baseline Evidence

## Goal

Document the current repo reality for PC Control so remediation work starts from
 measured facts, not inferred intent.

## Current Surface Map

Primary files:

- `dashboard/src/routes/pc-control/+page.svelte`
- `packages/sdk/src/pc-control.ts`
- `crates/ghost-gateway/src/api/pc_control.rs`
- `crates/ghost-gateway/src/bootstrap.rs`
- `crates/ghost-gateway/src/skill_catalog/definitions.rs`
- `crates/ghost-gateway/src/skill_catalog/service.rs`
- `crates/ghost-gateway/src/config_watcher.rs`
- `crates/ghost-pc-control/src/lib.rs`
- `crates/ghost-pc-control/src/safety/config.rs`
- `crates/ghost-pc-control/src/safety/input_validator.rs`

## Confirmed Current Behavior

### 1. Dashboard surface exists and is editable

The page can:

- toggle enabled state
- add/remove allowed apps
- add/remove blocked hotkeys
- draw safe zones
- view action log
- show budget bars
- show circuit-breaker state

### 2. Gateway exposes REST endpoints

Mounted endpoints:

- `GET /api/pc-control/status`
- `GET /api/pc-control/actions`
- `PUT /api/pc-control/status`
- `PUT /api/pc-control/allowed-apps`
- `PUT /api/pc-control/blocked-hotkeys`
- `PUT /api/pc-control/safe-zones`

### 3. SDK is wired to those endpoints

The SDK exposes:

- `getStatus()`
- `listActions()`
- `updateStatus()`
- `setAllowedApps()`
- `setBlockedHotkeys()`
- `setSafeZones()`

### 4. Runtime PC Control skills are compiled at bootstrap

Compiled skill definitions are built once during gateway startup from
`GhostConfig`, then stored in the skill catalog and used for execution
resolution.

### 5. Runtime enforcement is carried by skill-local state

PC Control skills hold:

- an `InputValidator`
- a shared `PcControlCircuitBreaker`
- backend instances

These are created from startup config and then reused for execution.

## Confirmed Gaps

## Gap 1: UI toggle is not a real live kill switch

Observed reality:

- the page writes `pc_control.enabled`
- runtime skills are registered at bootstrap based on config at that time
- no live rebuild or disable path exists for compiled PC Control skills

Impact:

- dashboard can display `Disabled` while already-registered skills remain
  executable
- this is safety-critical semantic drift

Severity:

- `P0`

## Gap 2: Safe-zone UI contract is false

Observed reality:

- page models `safe_zones: SafeZone[]`
- page supports multiple zones and zone labels
- API only accepts one effective safe zone
- config only persists one `safe_zone`
- readback rewrites the label to `Primary Safe Zone`

Impact:

- multi-zone editing is a fake affordance
- labels are not durable
- UI state is not an honest representation of persisted/runtime state

Severity:

- `P1`

## Gap 3: Budget display is semantically wrong

Observed reality:

- API status exposes `action_budget`
- that value is derived from audit row counts and
  `circuit_breaker.max_actions_per_second`
- actual skill gating also uses `pc_control.budgets`

Impact:

- operator sees a single budget concept
- product actually uses two different mechanisms
- UI can drive incorrect operational decisions

Severity:

- `P1`

## Gap 4: Websocket consumer wiring is incomplete

Observed reality:

- mutations emit `AgentConfigChange`
- PC Control page only listens to `AgentStateChange` and `Resync`

Impact:

- cross-tab and cross-operator edits do not reliably refresh the page
- config watcher events do not update this screen in normal operation

Severity:

- `P2`

## Gap 5: Config watcher path can diverge from API mutation path

Observed reality:

- config watcher uses env/default path discovery
- PC Control API writes `state.config_path`

Impact:

- watch path and write path can split under non-default startup modes
- hot reload becomes nondeterministic

Severity:

- `P2`

## Gap 6: Runtime state model is under-specified

Observed reality:

- status returns both `persisted` and `runtime`
- only circuit breaker state is currently represented as runtime
- validator config, enablement, and effective runtime policy are not surfaced

Impact:

- operators cannot distinguish intended config from effective runtime state
- debugging live drift is harder than necessary

Severity:

- `P2`

## Gap 7: Verification proves persistence, not runtime truth

Observed reality:

- current tests and live audits prove endpoint persistence and page rendering
- they do not prove that disabling PC Control revokes execution immediately
- they do not prove runtime config reload or honest safe-zone semantics

Impact:

- false confidence
- likely escape of safety regressions

Severity:

- `P1`

## Required Design Response

Any final design must solve:

1. live runtime reconfiguration
2. single source of truth for effective state
3. honest safe-zone contract
4. honest budget contract
5. coherent websocket refresh behavior
6. deterministic verification of live safety semantics
