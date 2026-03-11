# PC Control Verification Plan

**Version:** 1.0  
**Date:** March 11, 2026  
**Status:** Release Gate

## Goal

Define the evidence required to declare the PC Control remediation complete.

## Verification Philosophy

This subsystem is safety-sensitive. Persistence tests are necessary but
insufficient. Verification must prove:

- effective runtime behavior
- operator truthfulness
- cross-surface contract alignment

## Test Layers

## Layer 1: Unit tests

Required coverage:

- runtime snapshot normalization
- runtime apply revision increments
- runtime apply failure preserves prior snapshot
- singular safe-zone validation and clearing
- blocked-hotkey normalization
- status serializer correctness

## Layer 2: Gateway integration tests

Required coverage:

1. `status_toggle_disables_live_execution`
   - enable PC Control
   - resolve/execute representative PC Control skill
   - disable PC Control
   - re-execute representative skill
   - assert block without restart

2. `allowed_apps_update_is_live`
   - initial deny
   - update allowlist
   - assert allow after mutation

3. `blocked_hotkeys_update_is_live`
   - initial allow
   - add blocked hotkey
   - assert block after mutation

4. `safe_zone_update_is_live`
   - configure safe zone
   - assert click inside allowed, outside denied

5. `watcher_apply_updates_runtime`
   - external file edit on `state.config_path`
   - watcher triggers apply
   - status shows new runtime revision and state

6. `runtime_event_emitted_on_apply`
   - API mutation and watcher mutation both emit `PcControlRuntimeChange`

## Layer 3: SDK tests

Required coverage:

- typed status contract
- typed safe-zone contract
- backward compatibility behavior if plural helper remains

## Layer 4: Dashboard tests

Required coverage:

- page renders singular safe-zone editor only
- page reacts to runtime-change websocket event
- page separates runtime status from persisted status
- budget sections are semantically separated

## Layer 5: Live audit

Required live journey:

1. boot real gateway
2. load dashboard PC Control page
3. add allowlisted app
4. set safe zone
5. disable PC Control
6. verify page receives runtime-change event
7. execute representative PC Control skill via real route
8. assert execution is blocked
9. re-enable PC Control
10. assert expected allowed read-only execution path works

## Negative journeys

Must include:

- invalid safe-zone payload rejected
- watcher reads invalid YAML and preserves old runtime snapshot
- cross-tab mutation updates stale tab

## Evidence Artifacts

For live audit, collect:

- gateway logs
- websocket frames
- REST responses
- screenshots of page before and after mutation
- summary JSON with explicit pass/fail checks

## Exit Criteria

Release cannot pass unless all are true:

- all new tests pass
- live audit passes with no manual patching during run
- no known mismatch remains between:
  - config
  - API
  - SDK
  - dashboard
  - runtime execution

## Known Anti-Patterns To Reject

Do not accept any implementation that passes only because:

- the gateway was restarted after mutation
- the page mutated local state without server-confirmed runtime change
- compatibility aliases hide unresolved semantic disagreement
