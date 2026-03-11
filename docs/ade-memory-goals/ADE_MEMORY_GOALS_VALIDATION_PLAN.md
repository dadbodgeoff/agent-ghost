# ADE Memory + Goals Validation Plan

Purpose: define the minimum validation required before this work can be considered complete.

## 1. Storage Validation

- verify approved memory-write proposals create:
  - one proposal transition
  - one memory event
  - one latest memory snapshot
- verify approved goal-change proposals create or mutate durable goal state
- verify lineage-head state matches active goal state
- verify archive and unarchive update latest visible memory state correctly

## 2. API Validation

- `GET /api/goals` returns active durable goals, not just unresolved proposals
- proposal list endpoint returns pending and resolved proposals with correct states
- approved and auto-applied proposals are visible under the correct filters
- `GET /api/memory/:id` returns the same item the list and search surfaces reference
- memory search filters match canonical enum values

## 3. Runtime Validation

- runtime hydration sees newly approved goals without restart
- runtime hydration sees newly approved memory writes without restart
- reflection hydration remains intact after materialization unification
- no fallback path shows data that the ADE cannot inspect

## 4. Dashboard Validation

- goal detail cited-memory links navigate to specific memory detail
- memory detail route renders correct snapshot and metadata
- memory archive and unarchive actions update the UI correctly
- memory filters return expected rows
- goals tab distinguishes active goals from proposals
- goals tab status surfaces classify auto-applied items correctly
- websocket resync restores correct memory and goal state after reconnect

## 5. Contract Validation

- OpenAPI matches gateway behavior
- generated SDK types match OpenAPI
- hand-written SDK wrappers do not fork enum values or payload semantics
- dashboard consumers use SDK-exposed canonical values

## 6. Required Automated Tests

### Gateway tests

- approve goal proposal -> durable goal state exists
- auto-approve goal proposal -> durable goal state exists
- approve memory-write proposal -> memory event and snapshot exist
- memory search with canonical enum filters returns expected rows
- auto-applied proposals classify correctly in list filters

### Dashboard tests

- goals tab shows active goals and pending proposals separately
- proposal detail page links to memory detail
- memory detail route loads exact item
- archive/unarchive updates visible state
- websocket resync refreshes memory and goals surfaces

### SDK tests

- canonical status enums round-trip
- memory lifecycle methods hit correct routes
- goals API methods reflect final route semantics

## 7. Manual Verification Checklist

- create an agent proposal for a goal change
- verify it appears in pending proposals
- approve it
- verify active goals update without page reload
- open runtime execution or hydrated snapshot and verify the goal is present
- create a memory-write proposal
- approve it
- verify list, search, detail, and graph all surface it
- archive the memory
- verify it disappears from default list and search but remains retrievable by direct detail
- unarchive it
- verify it returns everywhere expected

## 8. Release Gates

This work does not pass release if any of the following are true:

- approval returns success before durable state exists
- runtime hydration and dashboard disagree on active goals
- memory filter values in the UI differ from canonical backend values
- a proposal references a memory that cannot be opened in the dashboard
- auto-approved items are misclassified by goals status filters
- any new route or SDK wrapper introduces contract drift not covered by generated types
