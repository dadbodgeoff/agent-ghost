# PC Control Contract Specification

**Version:** 1.0  
**Date:** March 11, 2026  
**Status:** Canonical Contract

## Scope

This document defines the required behavioral contract across:

- config
- gateway REST API
- websocket events
- SDK
- dashboard UI
- runtime execution

## 1. Product Contract

PC Control is an ADE subsystem that:

- governs whether desktop-control capabilities are operational
- exposes effective runtime state to operators
- enforces target-app, hotkey, safe-zone, and budget policies
- records executed and blocked actions

## 2. Safe-Zone Product Contract

For this remediation scope, the canonical contract is:

- exactly zero or one safe zone
- safe zone is a rectangle
- safe zone label is presentation-only and fixed to `Primary Safe Zone`

Required implications:

- dashboard must not allow multi-zone authoring
- API must expose singular semantics clearly
- SDK must support singular safe-zone write/read
- config persists one `safe_zone`

## 3. Enablement Contract

`pc_control.enabled` means:

- when `false`, PC Control execution is runtime-disabled immediately
- when `true`, PC Control execution is eligible, subject to all other gates

It does not mean:

- "will be disabled after restart"
- "persisted intent only"

## 4. Status Contract

`GET /api/pc-control/status` must return:

```json
{
  "persisted": {},
  "runtime": {},
  "telemetry": {}
}
```

Top-level mirror fields are not permitted unless they are explicitly documented
as convenience aliases and covered by tests. Preferred contract is no duplicate
top-level semantic fields.

### 4.1 Persisted section

Must contain:

- `enabled`
- `allowed_apps`
- `blocked_hotkeys`
- `safe_zone`
- `budgets`
- `circuit_breaker`

### 4.2 Runtime section

Must contain:

- `revision: string`
- `enabled: boolean`
- `activation_state: "active" | "disabled" | "degraded" | "apply_failed"`
- `effective_allowed_apps: string[]`
- `effective_blocked_hotkeys: string[]`
- `effective_safe_zone: SafeZone | null`
- `circuit_breaker_state: "closed" | "open" | "half_open"`
- `last_applied_at: string`
- `last_apply_source: "bootstrap" | "api" | "watcher" | "recovery"`

### 4.3 Telemetry section

Must contain:

- `throughput`
- `policy_budgets`
- `usage`
- `blocked_actions_recent`

`throughput` must cover rate/circuit-breaker counters.

`policy_budgets` must reflect configured budgets from `pc_control.budgets`.

`usage` must report observed counts aligned to those budgets.

## 5. Mutation Contract

## 5.1 Status toggle

`PUT /api/pc-control/status`

Request:

```json
{ "enabled": true }
```

Required behavior:

- validate payload
- persist config
- apply runtime change immediately
- emit runtime-change websocket event
- return fully updated status

## 5.2 Allowed apps

`PUT /api/pc-control/allowed-apps`

Request:

```json
{ "apps": ["Firefox", "VS Code"] }
```

Required behavior:

- replace full allowed-app list
- apply runtime immediately
- return updated status

## 5.3 Blocked hotkeys

`PUT /api/pc-control/blocked-hotkeys`

Request:

```json
{ "hotkeys": ["Cmd+Q", "Ctrl+Shift+Delete"] }
```

Required behavior:

- replace full blocked-hotkeys list
- normalize case semantics consistently
- apply runtime immediately

## 5.4 Safe zone

Preferred route for this remediation:

- keep `/api/pc-control/safe-zones` only if backward compatibility is required
- canonical request should still represent singular semantics

Canonical request:

```json
{ "safe_zone": { "x": 0, "y": 0, "width": 640, "height": 400 } }
```

or to clear:

```json
{ "safe_zone": null }
```

Compatibility handling:

- if `zones` is temporarily accepted, only zero or one entry may be used
- response and docs must present singular safe-zone truth

## 6. Websocket Contract

Add:

- `PcControlRuntimeChange`

Payload:

```json
{
  "type": "PcControlRuntimeChange",
  "revision": "uuid-or-monotonic-id",
  "enabled": false,
  "activation_state": "disabled",
  "change_source": "api",
  "changed_fields": ["enabled"]
}
```

Required client behavior:

- PC Control page must subscribe
- event should trigger a status refetch
- action log may refetch when relevant fields indicate execution-side change

## 7. SDK Contract

The SDK must expose semantics that match the canonical API.

Required methods:

- `getStatus()`
- `listActions(limit?)`
- `updateStatus(enabled, options?)`
- `setAllowedApps(apps, options?)`
- `setBlockedHotkeys(hotkeys, options?)`
- `setSafeZone(zone | null, options?)`

Temporary compatibility:

- `setSafeZones()` may remain during migration but must be deprecated and backed
  by singular semantics

## 8. Dashboard Contract

The dashboard must:

- render persisted and runtime state separately where ambiguity matters
- show one safe zone editor only
- not offer multi-zone labels or arrays
- subscribe to `PcControlRuntimeChange`
- display budget telemetry using distinct sections:
  - throughput
  - policy budgets
  - observed usage

The dashboard must not:

- imply runtime effect when only persisted state changed
- label unsupported user-defined zone names as durable
- compress different budget mechanisms into one bar

## 9. Runtime Execution Contract

Before any PC Control skill executes, runtime enforcement must check:

1. subsystem enabled
2. activation state permits execution
3. app policy
4. safe-zone policy
5. blocked-hotkey policy
6. budget policy
7. circuit-breaker policy

If subsystem is disabled:

- execution must fail deterministically
- failure must be represented as a PC Control policy block, not a silent miss

## 10. Audit Contract

Every mutation to PC Control operator config must create mutation audit records.

Every executed or blocked PC Control action must create forensic action rows.

Status reads are not required to write audit rows.

## 11. Compatibility Rules

Allowed during migration:

- temporary compatibility fields
- deprecated SDK helper aliases
- temporary dual-event emission

Not allowed:

- long-term semantic disagreement between singular and plural safe-zone models
- restart-required behavior for critical safety toggles
