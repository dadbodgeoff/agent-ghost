# Workflow ADE Agent Handoff

Status: March 11, 2026

Purpose: this is the operational handoff document to give an implementation agent so it can build the workflow ADE system start to finish without inventing architecture.

## Mission

Rebuild the workflow ADE surface into a fully coherent product across gateway, storage, SDK, WebSocket contracts, and dashboard.

The current system is not acceptable because the editor, SDK, runtime, and live status model disagree. Your job is to remove those disagreements and ship one truthful workflow system.

## Required Reading Order

Read these documents before changing code:

1. `docs/workflow-ade/WORKFLOW_ADE_MASTER_SPEC.md`
2. `docs/workflow-ade/WORKFLOW_ADE_CONTRACTS.md`
3. `docs/workflow-ade/WORKFLOW_ADE_IMPLEMENTATION_PLAN.md`
4. `docs/workflow-ade/WORKFLOW_ADE_VALIDATION_AND_ROLLOUT.md`

Treat them as binding.

## Non-Negotiable Rules

- Do not preserve dead UI features for convenience.
- Do not add new workflow semantics unless they are reflected in shared contracts.
- Do not let dashboard workflow types fork from backend reality.
- Do not leave recovery functionality backend-only.
- Do not ship generic workflow status events that require ADE to guess the node identity.
- Do not mark the work complete until the release gates pass.

## Implementation Order

Execute the work in this order:

1. Freeze and implement the canonical workflow contracts.
2. Update persistence and execution detail support.
3. Complete runtime semantics for the truthful production-cut node kinds only.
4. Extend the SDK for full workflow lifecycle coverage.
5. Rebuild the ADE workflow page to match runtime truth.
6. Add and pass validation gates.

## Required Deliverables

You are done only when all of the following exist and work:

- canonical typed workflow schema in backend/OpenAPI/SDK/dashboard
- truthful node palette and config UI
- complete execution history and detail support
- truthful recovery handling in ADE, with resume controls only when the recovery action is machine-resumable
- workflow-specific WebSocket live events
- tests and live audit coverage for full workflow lifecycle

## Primary Code Targets

Focus first on:

- `crates/ghost-gateway/src/api/workflows.rs`
- `crates/ghost-gateway/src/api/openapi.rs`
- `crates/ghost-gateway/src/api/websocket.rs`
- `crates/cortex/cortex-storage/src/queries/workflow_execution_queries.rs`
- migrations under `crates/cortex/cortex-storage/src/migrations/`
- `packages/sdk/src/workflows.ts`
- `packages/sdk/src/websocket.ts`
- `dashboard/src/routes/workflows/+page.svelte`
- `dashboard/src/components/WorkflowCanvas.svelte`
- `dashboard/src/components/WorkflowNodeConfig.svelte`
- `dashboard/scripts/live_surface_audit.mjs`

## Explicit Success Criteria

The implementation is successful only if:

- every workflow node type shown in ADE can execute successfully through the runtime
- ADE does not advertise branch, sub-workflow, parallel, or A/B semantics unless the runtime implements them
- node-level live execution state appears correctly in ADE during a run
- execution history and execution detail are accessible from ADE
- recovery-required runs can be understood and resumed from ADE when allowed
- contract drift is caught by tests or gates

## Stop Conditions

Stop and resolve the issue before proceeding if:

- you discover contract ambiguity not covered by the package
- existing dirty workspace changes conflict directly with workflow files you must edit
- a proposed implementation requires contradicting the master spec

## Recommended Working Style

- implement phase by phase
- keep contracts explicit
- prefer extraction over growing `api/workflows.rs` into an untestable monolith
- add tests as each phase lands instead of deferring all validation to the end

## Final Verification Checklist

Before declaring completion, prove:

- contract tests pass
- backend workflow tests pass
- SDK tests pass
- dashboard workflow tests pass
- live-surface workflow audit passes

If any one of these does not pass, the workflow rebuild is not finished.
