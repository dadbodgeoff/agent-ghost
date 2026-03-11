# PC Control Target Architecture

**Version:** 1.0  
**Date:** March 11, 2026  
**Status:** Normative Architecture

## Architectural Objective

PC Control must become a first-class ADE control plane subsystem, not a thin
config editor over startup-time skill registration.

The subsystem must support:

- truthful operator state
- immediate safety effect for critical toggles
- deterministic runtime reconciliation
- aligned contracts across UI, API, SDK, config, and execution
- release-grade verification

## Core Decision

Introduce a dedicated runtime owner:

- `PcControlRuntimeService`

This service becomes the canonical owner of effective PC Control runtime state.

## Ownership Model

### Source of truth layers

1. `Persisted intent`
   - `ghost.yml`
   - operator-declared desired configuration

2. `Applied runtime snapshot`
   - in-memory state owned by `PcControlRuntimeService`
   - includes revision, effective policy, and activation status

3. `Execution-time enforcement`
   - skill resolution and execution checks routed through the runtime service
   - must use the current applied snapshot, not stale bootstrap copies

### Forbidden model

The following model is explicitly disallowed:

- config writes that do not update live runtime behavior
- UI that infers runtime truth from persisted YAML alone

## Structural Design

## 1. Runtime service

Create a gateway-local service with responsibilities:

- load PC Control config from `state.config_path`
- validate and normalize config into a runtime snapshot
- maintain a monotonic runtime revision
- expose current effective state to API handlers
- apply critical state transitions atomically
- rebuild or rebind runtime skill enforcement dependencies
- emit websocket events when effective runtime changes

Suggested components:

- `PcControlRuntimeConfig`
- `PcControlRuntimeSnapshot`
- `PcControlRuntimeService`
- `PcControlRuntimeApplyResult`

## 2. Execution integration

PC Control skill execution must no longer depend on bootstrap-frozen validator
and enablement semantics.

There are two acceptable implementation paths:

### Path A: Runtime-owned enforcement handle

Each PC Control skill holds a lightweight handle to runtime state and resolves
effective validator/circuit-breaker policy on execution.

Benefits:

- avoids full skill catalog rebuild
- live policy changes become immediate

Risks:

- requires careful shared-state design

### Path B: Atomic skill catalog rebuild on runtime apply

Every PC Control runtime config change rebuilds the relevant compiled skill
definitions and swaps the catalog/runtime references atomically.

Benefits:

- simpler conceptual alignment with current architecture

Risks:

- heavier churn
- easier to get partial updates wrong

Preferred direction:

- `Path A`

Reason:

- PC Control is policy-heavy and safety-sensitive
- policy should be mutable without rebuilding unrelated runtime structures

## 3. Config reconciliation

All config entry points must converge into one apply pipeline:

- dashboard mutation endpoints
- direct file edits detected by watcher
- future CLI/admin mutation flows

Required flow:

1. read candidate config from `state.config_path`
2. validate candidate PC Control config
3. construct normalized runtime snapshot
4. atomically swap runtime state
5. emit websocket event with new runtime revision
6. expose new effective state via status endpoint

## 4. Status model

Status must separate:

- persisted intent
- effective runtime
- derived telemetry

Required status sections:

- `persisted`
- `runtime`
- `telemetry`

`runtime` must include at minimum:

- `revision`
- `enabled`
- `activation_state`
- `effective_allowed_apps`
- `effective_blocked_hotkeys`
- `effective_safe_zones`
- `circuit_breaker_state`
- `runtime_last_applied_at`
- `runtime_last_apply_source`

`telemetry` must include:

- circuit-breaker throughput counters
- session/action budget counters
- recent blocked-action counts

## 5. Safe-zone model

Choose one and only one product contract:

### Option A: single primary safe zone

- simplest
- aligns to current config model
- requires UI simplification

### Option B: multiple labeled safe zones

- richer operator model
- requires config, API, SDK, validator, UI, tests, and migration support

Preferred direction:

- `Option A` for immediate remediation

Reason:

- current runtime and config already assume one region
- fastest path to an honest and coherent system
- multi-zone support can be a later intentional feature

## 6. Budget model

The system must distinguish:

1. `Rate limit / circuit-breaker throughput`
   - time-window protection against runaway execution

2. `Policy execution budgets`
   - configured action budgets by skill/action family

3. `Observed usage`
   - actual recent counts from audit/action telemetry

These must never again be compressed into one ambiguous field.

## 7. Event model

Add explicit event semantics for PC Control runtime change.

Preferred event:

- `PcControlRuntimeChange`

It should contain:

- `revision`
- `change_source`
- `changed_fields`
- `enabled`
- `activation_state`

`AgentConfigChange` may still be emitted for broad config consumers, but PC
Control clients must not depend on generic agent-state events for refresh.

## 8. Config watcher design

The config watcher must use `state.config_path`, not independent path discovery,
for runtime apply of PC Control state.

The watcher should:

- validate file change
- apply through `PcControlRuntimeService`
- emit the same runtime event path as API mutations

## 9. Failure behavior

Runtime apply failures must be explicit.

If a new config cannot be applied:

- persisted file write may be rejected before commit for API mutations
- runtime should retain prior valid snapshot
- status should expose apply failure metadata
- websocket/system warning should surface failure

No silent partial apply is acceptable.

## Sequence: Operator Mutation

```text
Dashboard -> Gateway PC Control API
  -> validate request
  -> update ghost.yml
  -> PcControlRuntimeService.apply_from_config("api")
  -> atomic runtime snapshot swap
  -> emit PcControlRuntimeChange
  -> status endpoint returns new effective state
  -> dashboard refetches and renders
```

## Sequence: External Config Edit

```text
File edit -> Config watcher
  -> read state.config_path
  -> validate file
  -> PcControlRuntimeService.apply_from_config("watcher")
  -> atomic runtime snapshot swap
  -> emit PcControlRuntimeChange
  -> dashboard refetches and renders
```

## Acceptance Characteristics

The architecture is correct only if all are true:

- disabling PC Control blocks subsequent PC Control execution without restart
- status endpoint can prove effective runtime state
- page never shows unsupported multiplicity
- websocket-driven refresh works cross-tab and cross-operator
- config watcher and API mutations converge on the same runtime apply path
