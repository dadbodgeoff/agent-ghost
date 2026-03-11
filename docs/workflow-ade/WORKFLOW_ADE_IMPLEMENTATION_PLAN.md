# Workflow ADE Implementation Plan

Status: March 11, 2026

Purpose: sequence the implementation work required to bring the workflow ADE surface to the target state defined in the master spec and contracts.

This plan is written to be executable by an implementation agent.

## Delivery Strategy

Implement in six phases:

1. Contract freeze
2. Backend model and persistence
3. Execution runtime completion for the truthful production cut
4. SDK completion
5. ADE completion
6. Validation and release hardening

No phase should begin if it depends on unresolved contract decisions from an earlier phase.

## Phase 0: Preconditions

Before code changes:

- read the full workflow package in `docs/workflow-ade/`
- verify current git status and avoid reverting unrelated changes
- treat the current live workflow audit findings as authoritative

Definition of done:

- no unresolved contract ambiguity remains

## Phase 1: Contract Freeze

Objective: make the workflow schema explicit and shared.

Primary targets:

- `crates/ghost-gateway/src/api/workflows.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `packages/sdk/src/generated-types.ts`
- `packages/sdk/src/workflows.ts`
- dashboard workflow types

Tasks:

1. Introduce typed workflow structs/enums for supported node kinds, config shapes, execution status, and recovery status.
2. Stop parsing workflow definitions as free-form arrays of generic JSON values inside execution logic.
3. Add an explicit validation helper contract, and only add a validation route if ADE needs it.
4. Remove the implicit top-level node runtime field pattern and normalize all runtime configuration into `config`.
5. Decide and implement legacy normalization behavior for existing rows.

Definition of done:

- backend has typed workflow contracts
- OpenAPI reflects them
- SDK types derive from them cleanly
- no remaining executor path depends on raw untyped node fields

## Phase 2: Backend Model and Persistence

Objective: support execution detail, event history, and explicit recovery state.

Primary targets:

- `crates/cortex/cortex-storage/src/migrations/`
- `crates/cortex/cortex-storage/src/queries/workflow_execution_queries.rs`
- `crates/ghost-gateway/src/api/workflows.rs`

Tasks:

1. Extend or replace persistence to store:
   - workflow version used by execution
   - normalized graph snapshot
   - node states with typed statuses
   - execution-level recovery data
2. Add a durable workflow execution event log table if current storage is insufficient for detail and replay.
3. Add a `GET /api/workflows/:id/executions/:execution_id` endpoint.
4. Ensure timestamps are RFC 3339 UTC everywhere.
5. Ensure recovery-required state can be loaded and explained without re-running the execution.

Definition of done:

- one execution can be fully inspected after completion or crash
- history, detail, and recovery state are queryable from storage

## Phase 3: Execution Runtime Completion

Objective: align runtime semantics with visible ADE features.

Primary targets:

- `crates/ghost-gateway/src/api/workflows.rs`
- any extracted runtime module created from it
- tool/runtime integration points

Tasks:

1. Complete runtime support for the production-cut node set without exposing unimplemented orchestration semantics.
2. Implement runtime handlers for:
   - `llm_call`
   - `tool_exec`
   - `gate_check`
   - `transform`
   - `condition`
   - `wait`
3. Keep edge semantics truthful:
   - edges define dependency order
   - edge metadata is treated as visual only in this cut
   - condition nodes act as linear predicates, not branch routers
4. Emit workflow-specific WebSocket events with node identifiers.
5. Persist node-level transition events as they happen when storage detail requires it.

Definition of done:

- every visible workflow node type is executable
- ADE does not claim branch, sub-workflow, parallel, or A/B behavior that the runtime does not implement
- node-level live events are emitted with enough detail for ADE

## Phase 4: SDK Completion

Objective: expose the full workflow lifecycle to clients.

Primary targets:

- `packages/sdk/src/workflows.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/index.ts`

Tasks:

1. Add SDK methods for:
   - list executions
   - get execution detail
   - resume execution
2. Align hand-written SDK wrappers with generated types.
3. Add typed workflow WebSocket event variants.
4. Add SDK tests for workflow routes and WebSocket payload parsing.

Definition of done:

- ADE can implement the full workflow lifecycle without bypassing the SDK

## Phase 5: ADE Completion

Objective: make the dashboard a truthful operator surface for workflow authoring and execution.

Primary targets:

- `dashboard/src/routes/workflows/+page.svelte`
- `dashboard/src/components/WorkflowCanvas.svelte`
- `dashboard/src/components/WorkflowNodeConfig.svelte`
- workflow-related dashboard stores as needed

Tasks:

1. Reduce or replace node palette items to match supported runtime semantics exactly.
2. Update node config forms to edit canonical `config` fields for each supported type.
3. Add graph validation UI:
   - save-time validation
   - execute-time validation
   - inline validation errors
4. Add execution history panel.
5. Add execution detail panel or route.
6. Add resume controls only for recovery-required executions whose recovery action is machine-resumable.
7. Subscribe to typed workflow WebSocket events and update node-level live state.
8. Render explicit failure reasons and recovery actions.
9. Make workflow execution result UX human-readable; raw JSON blob is insufficient.

Definition of done:

- ADE can author, validate, execute, inspect, and handle recovery truthfully without hidden backend-only capabilities

## Phase 6: Validation and Release Hardening

Objective: prevent recurrence of the same class of mismatch.

Primary targets:

- gateway integration tests
- SDK tests
- dashboard tests
- `dashboard/scripts/live_surface_audit.mjs`
- parity or contract scripts

Tasks:

1. Add contract tests proving backend, OpenAPI, SDK, and dashboard consume the same workflow schema.
2. Add gateway tests for every supported node kind and the current linear execution semantics.
3. Add dashboard tests for:
   - authoring each node type
   - validation errors
   - execution history
   - live node status
   - resume flow
4. Extend live-surface audit to cover:
   - create
   - update
   - execute
   - execution history
   - execution detail
   - resume recovery-required execution
5. Add a regression gate that fails if dashboard node palette includes unsupported node types.

Definition of done:

- the old class of editor/runtime drift is mechanically gated

## File-Level Target Map

Expected primary modifications:

- `crates/ghost-gateway/src/api/workflows.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/cortex/cortex-storage/src/queries/workflow_execution_queries.rs`
- new workflow execution event query/storage files if introduced
- new migrations under `crates/cortex/cortex-storage/src/migrations/`
- `packages/sdk/src/workflows.ts`
- `packages/sdk/src/websocket.ts`
- `packages/sdk/src/index.ts`
- `dashboard/src/routes/workflows/+page.svelte`
- `dashboard/src/components/WorkflowCanvas.svelte`
- `dashboard/src/components/WorkflowNodeConfig.svelte`
- `dashboard/scripts/live_surface_audit.mjs`

## Sequencing Constraints

- Do not extend the UI first.
- Do not implement live visualization before typed workflow events exist.
- Do not implement recovery UI before execution detail exists.
- Do not keep legacy and target node semantics both user-visible unless migration UX is explicit.

## Definition of Finished

The implementation is finished only when all phases are complete and the release gates in `WORKFLOW_ADE_VALIDATION_AND_ROLLOUT.md` pass without exceptions.
