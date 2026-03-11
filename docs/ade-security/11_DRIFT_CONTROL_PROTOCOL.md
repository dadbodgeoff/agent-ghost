# ADE Security Remediation: Drift Control Protocol

Status: draft for implementation orchestration on March 11, 2026.

This protocol exists to stop local fixes from reintroducing cross-surface drift.

## 1. Pre-Change Checklist

Before starting any work package:

1. confirm the package exists in `09_EXECUTION_TRACKER.md`
2. confirm dependencies are not `Blocked`
3. inspect the current contract owner files
4. identify all cross-surface consumers
5. decide whether the package changes:
   - REST contract
   - SDK type
   - shell affordance
   - page affordance
   - websocket refresh behavior
   - export semantics

If any answer is yes, all affected surfaces must be included in the package
scope before coding starts.

## 2. Post-Change Drift Checklist

After coding and before marking `Code Complete`:

1. verify gateway and SDK types still align
2. verify shell and page permission behavior still align
3. verify query and export semantics still align
4. verify websocket refresh still updates all impacted state
5. verify error, empty, and partial states remain distinct

## 3. Required Drift Questions

Ask these after each package:

- Did this change add a new canonical field?
- Did this change leave old compatibility parsing in place?
- Did this change alter the list of valid event types?
- Did this change alter the list of valid severities?
- Did this change make an action visible in one place but not another?
- Did this change update query semantics without export semantics?

If any answer is yes, update the contract docs and tracker before moving on.

## 4. Mandatory Drift Surfaces

For this remediation, drift must be checked across:

- gateway REST payloads
- SDK types
- dashboard route rendering
- shell shortcuts
- command palette
- websocket-driven refresh
- export behavior

## 5. Stop Conditions

Stop and re-scope immediately if:

- a contract change cannot be applied consistently across all consumers
- a package requires changing backend semantics that would invalidate earlier
  accepted tests
- unrelated repo changes create conflicting behavior in any mandatory drift
  surface

## 6. Enforcement Rule

No package is `Verified` unless the post-change drift checklist is explicitly
passed.
