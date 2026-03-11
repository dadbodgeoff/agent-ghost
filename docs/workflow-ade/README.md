# Workflow ADE Build Package

Status: March 11, 2026

Purpose: define the complete document package required to rebuild the ADE workflow surface into a coherent, truthful, production-grade system that can be handed to an implementation agent and executed start to finish without architectural guesswork.

This package now documents the truthful production cut, not an aspirational workflow engine. As of March 11, 2026, the supported runtime node set is:

- `llm_call`
- `tool_exec`
- `gate_check`
- `transform`
- `condition`
- `wait`

Explicit branch routing, parallel fork/join, sub-workflow invocation, and A/B routing are follow-on work and must not be reintroduced into ADE until they are implemented end to end.

This package exists because the current workflow surface is only partially real:

- the editor exposes node types the runtime does not support
- branch semantics are richer in the UI than in execution
- node configuration shape is inconsistent between UI and executor
- durable recovery exists in the gateway but is not fully exposed through the SDK and ADE
- live execution visualization is claimed by the UI but is not actually wired end to end

## Package Contents

Read these documents in order:

1. `WORKFLOW_ADE_MASTER_SPEC.md`
2. `WORKFLOW_ADE_CONTRACTS.md`
3. `WORKFLOW_ADE_IMPLEMENTATION_PLAN.md`
4. `WORKFLOW_ADE_VALIDATION_AND_ROLLOUT.md`
5. `WORKFLOW_ADE_AGENT_HANDOFF.md`

## Authority Rules

If documents conflict, precedence is:

1. `WORKFLOW_ADE_MASTER_SPEC.md`
2. `WORKFLOW_ADE_CONTRACTS.md`
3. `WORKFLOW_ADE_IMPLEMENTATION_PLAN.md`
4. `WORKFLOW_ADE_VALIDATION_AND_ROLLOUT.md`
5. `WORKFLOW_ADE_AGENT_HANDOFF.md`

The handoff document is operational, not architectural. It summarizes and sequences the package, but it does not override the spec or contracts.

## Target Outcome

When this package is fully implemented, the workflow surface must satisfy all of the following:

- every workflow feature shown in ADE is actually executable in the current production cut
- every executable behavior is represented in the shared contract, SDK, backend, and dashboard
- execution state is durable, inspectable, resumable, and observable
- workflow authoring, execution, history, recovery, and live status are one coherent product surface
- tests and release gates fail closed on contract drift or dead UI/runtime paths

## Primary Code Sources

This package is grounded in the current live implementation, especially:

- `crates/ghost-gateway/src/api/workflows.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/ghost-gateway/src/route_sets.rs`
- `crates/cortex/cortex-storage/src/queries/workflow_execution_queries.rs`
- `crates/cortex/cortex-storage/src/migrations/v021_workflows.rs`
- `crates/cortex/cortex-storage/src/migrations/v040_phase3_tables.rs`
- `crates/cortex/cortex-storage/src/migrations/v056_workflow_execution_contract.rs`
- `packages/sdk/src/workflows.ts`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/routes/workflows/+page.svelte`
- `dashboard/src/components/WorkflowCanvas.svelte`
- `dashboard/src/components/WorkflowNodeConfig.svelte`
- `dashboard/scripts/live_surface_audit.mjs`

## Use

If you are handing implementation to an agent, hand over `WORKFLOW_ADE_AGENT_HANDOFF.md` and keep the rest of this package in scope as referenced authority.
