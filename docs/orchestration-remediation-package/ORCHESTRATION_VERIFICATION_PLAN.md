# Orchestration Verification Plan

Status: March 11, 2026
Purpose: define the gates that must pass before orchestration remediation is considered complete.

## Verification Standard

- No orchestration contract change without backend integration coverage.
- No dashboard orchestration refactor without route-level interaction coverage.
- No realtime claim without reconnect/resync testing.
- No graph or metric contract without topology-based fixtures.
- No final handoff accepted if any major panel can still show stale or contradictory state.

## Gate Matrix

## Gate 1. Backend Read Model Tests

### Trust Graph

Must prove:

- trust nodes render from registered agents
- trust edges render from valid seeded delegation state
- invalid schema references do not fail silently
- empty relationship graph returns empty edges for legitimate reasons only

### Consensus

Must prove:

- pending, approved, rejected, superseded, and timed-out states map correctly
- counts are derived from real records only
- threshold semantics are deterministic and documented

### Sybil

Must prove:

- chain depth changes with deeper delegation chains
- fan-out/concentration metrics change under different graph shapes
- cycles or suspicious structures are surfaced if modeled

### A2A

Must prove:

- discovery preserves configured peers
- verified and unverified peers are distinguished
- dispatch creates durable task records
- task lifecycle updates reconcile correctly when update sources exist

## Gate 2. SDK Contract Tests

Must prove:

- `packages/sdk/src/mesh.ts` returns payloads matching backend schemas
- `packages/sdk/src/a2a.ts` returns payloads matching backend schemas
- websocket typed events cover orchestration event flow

## Gate 3. Dashboard Store Tests

Must prove:

- one orchestration store owns the authoritative state
- child components do not fetch critical orchestration state independently
- websocket events patch or refetch the correct store slices
- resync refreshes all required orchestration panels

## Gate 4. Dashboard Interaction Tests

Must prove:

- trust graph updates after orchestration-relevant changes
- consensus tab reflects state changes after proposal transitions
- A2A header count and task table remain aligned
- clicking a discovered agent can flow into task dispatch without manual copy/paste
- reconnect/resync recovers coherent page state

## Gate 5. Failure-Mode Tests

Must prove:

- backend query/read-model failure is surfaced rather than silently rendered as empty truth
- A2A remote failure produces truthful task state
- websocket disconnect and reconnect do not leave partial stale slices
- discovery failure does not erase prior known peers

## Recommended Test File Targets

- `crates/ghost-gateway/tests/orchestration_api_tests.rs` (new)
- `packages/sdk/src/__tests__/client.test.ts`
- `dashboard/tests/orchestration.spec.ts` (new)

## Test Data Requirements

Create deterministic fixtures for:

- no agents
- two agents with one delegation
- three agents with a chain of delegations
- high fan-out delegator
- proposals in multiple lifecycle states
- configured peer plus discovered peer
- verified peer and unverified peer
- A2A task in submitted, working, completed, failed states where modeled

## CI Requirement

This package expects orchestration tests to become blocking CI gates. A remediation that only passes local manual inspection is incomplete.

## Definition Of Done

Orchestration work is only done when:

- all backend integration tests pass
- all SDK contract tests pass
- all dashboard orchestration tests pass
- the page remains coherent under live update and resync scenarios
- no panel still relies on placeholder or silently degraded truth
