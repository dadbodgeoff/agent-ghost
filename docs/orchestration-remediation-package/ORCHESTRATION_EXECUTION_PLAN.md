# Orchestration Execution Plan

Status: March 11, 2026
Purpose: define the implementation sequence, work packages, dependencies, file targets, and acceptance criteria.

## Delivery Principle

Do not start with dashboard cosmetics. Start by making backend truth correct, then make SDK truth match, then unify frontend state, then close gaps with tests.

## Phase Graph

Phase 1: Truth Repair
- 1.1 trust graph backend repair
- 1.2 consensus backend repair
- 1.3 sybil/delegation backend repair
- 1.4 A2A lifecycle contract repair

Phase 2: Contract Alignment
- 2.1 SDK type alignment
- 2.2 websocket event alignment
- 2.3 API/OpenAPI parity coverage

Phase 3: Dashboard State Unification
- 3.1 orchestration store creation
- 3.2 route migration
- 3.3 component demotion to presentational roles
- 3.4 deep-link and workflow cohesion

Phase 4: Verification And Hardening
- 4.1 backend integration tests
- 4.2 SDK contract tests
- 4.3 dashboard tests
- 4.4 failure-path and resync tests

## Phase 1.1 Trust Graph Backend Repair

### Goal

Replace the current trust edge query with a schema-valid, semantics-valid derivation path.

### Target Files

- `crates/ghost-gateway/src/api/mesh_viz.rs`
- optional new helper module under `crates/ghost-gateway/src/api/`
- `crates/cortex/cortex-storage/src/queries/` if shared queries are extracted

### Work

- define what a trust edge means in ADE today
- map persisted delegation/current-head state to trust graph edges
- compute edge weights from explicit derivation rules
- stop swallowing query preparation failure
- add backend-level trace/error reporting for invalid read model derivation

### Acceptance

- edge query uses existing schema only
- edge set is non-empty in seeded delegation scenarios
- empty edge set is distinguishable from read-model failure
- trust graph response is integration-tested

## Phase 1.2 Consensus Backend Repair

### Goal

Make consensus reflect the actual proposal lifecycle model rather than a fake count over legacy rows.

### Target Files

- `crates/ghost-gateway/src/api/mesh_viz.rs`
- `crates/cortex/cortex-storage/src/queries/goal_proposal_queries.rs`
- optional new query helpers

### Work

- define what "consensus" means in current ADE
- derive status from v2 proposal/transition state
- either:
  - expose real review/transition counts, or
  - reduce the panel to truthful lifecycle state if vote counts do not exist
- document threshold semantics explicitly

### Acceptance

- no count in API payload is fabricated
- panel status and goals/proposals status agree for the same entity
- route is covered by integration tests with multiple proposal states

## Phase 1.3 Sybil And Delegation Backend Repair

### Goal

Promote sybil posture from placeholder counts to graph-derived metrics.

### Target Files

- `crates/ghost-gateway/src/api/mesh_viz.rs`
- optional new graph-analysis helper module

### Work

- define whether `delegations` returns current state heads or transition history
- compute active delegation graph
- compute max chain depth
- compute fan-out and concentration indicators
- add at least one structural risk indicator

### Acceptance

- `max_chain_depth` is computed
- metrics change meaningfully across seeded topology scenarios
- terminology in payload and UI matches actual semantics

## Phase 1.4 A2A Lifecycle Contract Repair

### Goal

Make A2A discovery and task execution a coherent subsystem rather than a submit-only surface.

### Target Files

- `crates/ghost-gateway/src/api/a2a.rs`
- `crates/cortex/cortex-storage/src/migrations/v028_a2a_tasks.rs`
- new migration if new fields are required
- `crates/ghost-gateway/src/api/websocket.rs`

### Work

- add `updated_at` and any needed lifecycle metadata to `a2a_tasks`
- distinguish discovery source and verification state
- choose a real reconciliation mechanism for remote task state
- emit websocket events for meaningful lifecycle transitions, not just submission

### Acceptance

- successful tasks can move past `submitted` where remote data is available
- state model is explicit in code and tests
- discovery result explains configured versus discovered versus verified state

## Phase 2.1 SDK Type Alignment

### Goal

Make SDK orchestration types authoritative wrappers over the fixed backend contracts.

### Target Files

- `packages/sdk/src/mesh.ts`
- `packages/sdk/src/a2a.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/generated-types.ts`

### Work

- align type aliases with backend payloads
- remove or document any intentional wrapper deviations
- extend websocket union with orchestration-relevant event types if needed

### Acceptance

- SDK orchestration calls compile directly against generated types or explicit exceptions
- websocket event typing covers orchestration flows

## Phase 2.2 API/OpenAPI Parity Coverage

### Goal

Prevent future orchestration drift.

### Target Files

- `crates/ghost-gateway/src/api/openapi.rs`
- `scripts/check_openapi_parity.py`
- any orchestration-specific parity scripts if required

### Work

- ensure orchestration endpoints are represented with concrete schemas
- strengthen parity gates beyond route presence when needed

### Acceptance

- orchestration routes are schema-checked in CI
- payload drift can fail a gate

## Phase 3.1 Orchestration Store Creation

### Goal

Move orchestration state into one store.

### Target Files

- new `dashboard/src/lib/stores/orchestration.svelte.ts`
- `dashboard/src/routes/orchestration/+page.svelte`

### Work

- centralize loading, reload, and websocket subscriptions
- expose route-friendly slice accessors and actions
- implement targeted reload and resync behavior

### Acceptance

- route does not fetch orchestration data directly except through store APIs
- all panels read from one shared state owner

## Phase 3.2 Route Migration And Component Cleanup

### Goal

Make orchestration components presentational and action-focused.

### Target Files

- `dashboard/src/routes/orchestration/+page.svelte`
- `dashboard/src/components/A2ATaskTracker.svelte`
- `dashboard/src/components/A2AAgentCard.svelte`

### Work

- remove child-owned fetches
- pass task list and action callbacks as props
- wire discovered peer cards to task form or direct-send actions
- ensure trust/consensus/sybil panels react to live store updates

### Acceptance

- no contradictory A2A task count versus row list
- discovered peers can be acted on directly
- route remains coherent after websocket events and resync

## Phase 3.3 Cross-ADE Cohesion

### Goal

Connect orchestration to the rest of ADE rather than leaving it as an isolated page.

### Target Files

- `dashboard/src/routes/orchestration/+page.svelte`
- relevant route components for deep links

### Work

- add links from proposals to goals surface
- add links from agents to agent detail
- add links from sessions/workflows where relevant

### Acceptance

- every major orchestration artifact shown in the UI can be traced to its owning ADE surface

## Phase 4 Verification

This phase is blocked until Phases 1 through 3 are complete enough to test honestly.

See `ORCHESTRATION_VERIFICATION_PLAN.md` for the detailed gate set.

## Sequencing Rule

The implementation agent must complete work in this order:

1. backend truth repair
2. SDK/OpenAPI alignment
3. dashboard store unification
4. component cleanup
5. verification

Any attempt to lead with UI polish before backend correctness is a spec violation.
