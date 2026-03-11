# Workflow ADE Master Spec

Status: March 11, 2026

Purpose: define the authoritative target design for the ADE workflow system across dashboard authoring, SDK contracts, gateway APIs, execution runtime, storage, recovery, WebSocket updates, and validation gates.

This document is based on the live codebase. If this spec conflicts with older workflow sections in `GHOST_ADE_IMPLEMENTATION.md`, `design.md`, or dashboard comments, this spec wins.

## Standard

This work is held to the following non-negotiable bar:

- No workflow feature may be visible in ADE unless it is fully executable.
- No runtime behavior may exist without one canonical contract shared by backend, SDK, and dashboard.
- No node schema field may be interpreted differently by authoring, persistence, and execution.
- No execution state may exist only in memory if ADE claims durability or recovery.
- No live status surface may claim real-time behavior without a typed end-to-end event path.
- No recovery feature may exist only at the REST layer if ADE is the primary operator surface.
- No tests may stop at route presence where semantic drift remains possible.

## Package-Starting Defects

This package was created because the workflow surface had the following verified defects:

1. The dashboard exposes `tool_exec`, `parallel_branch`, `sub_workflow`, and `ab_test`, but the executor rejects them as unknown node types.
2. The editor models branch metadata, but the runtime executes a plain topological walk and ignores branch semantics.
3. The config UI writes to `node.config`, while runtime-critical fields are read from top-level node keys.
4. Execution history and resume endpoints exist in the gateway but are not fully surfaced in the SDK and ADE.
5. The workflow canvas claims live execution visualization but does not subscribe to any usable workflow event stream.
6. The current workflow tests emphasize API durability and route presence more than end-to-end product fidelity.

## Product Definition

The workflow surface is one product, not separate editor and executor subsystems.

The product includes:

- workflow list and detail navigation
- authoring and editing
- graph validation
- workflow execution
- execution history
- execution detail
- crash recovery and manual resume
- live status visualization
- operator-visible failure reasons
- typed SDK access
- contract and regression gates

ADE must be able to answer these operator questions without leaving the product surface:

- What workflows exist?
- What does this workflow do?
- Is this workflow valid?
- What happened during the last run?
- Which node failed?
- Can I resume it?
- Is the graph I see the graph the engine will run?
- Are the limited semantics in this production cut actually real?

## Scope

In scope:

- `dashboard/src/routes/workflows/+page.svelte`
- `dashboard/src/components/WorkflowCanvas.svelte`
- `dashboard/src/components/WorkflowNodeConfig.svelte`
- workflow-facing dashboard stores as needed
- `packages/sdk/src/workflows.ts`
- `packages/sdk/src/websocket.ts`
- `crates/ghost-gateway/src/api/workflows.rs`
- workflow OpenAPI declarations
- workflow WebSocket event declarations
- workflow execution persistence
- workflow execution event persistence if needed
- contract, integration, dashboard, and live-surface gates

Out of scope:

- replacing the broader ADE shell
- redesigning unrelated studio, session, or agent surfaces
- building a generic BPMN engine
- introducing external orchestration infrastructure before the local workflow contract is correct

## Target End State

### 1. One Canonical Workflow Contract

There must be one shared schema for:

- workflow definition
- workflow node
- workflow edge
- execution state
- execution event
- execution status
- recovery status

The same schema must drive:

- gateway request parsing
- OpenAPI generation
- SDK types
- dashboard types
- persistence serialization
- executor normalization

### 2. Truthful Authoring Surface

The workflow editor must only expose node types and fields that the runtime actually supports.

The graph shown in ADE must match runtime semantics exactly:

- tool execution nodes must execute through a real tool-binding path
- unsupported node types must not be creatable

### 3. Normalized Runtime Model

Execution must operate on a normalized graph model, not ad hoc raw JSON. The runtime must:

- validate graph structure before persistence and before execution
- normalize node config before execution
- reject schema drift early
- emit deterministic node-level status transitions
- persist enough state to resume retry-safe progress
- record enough detail to explain failures in ADE

### 4. First-Class Execution Lifecycle

The workflow product must support:

- create
- update
- validate
- execute
- inspect history
- inspect one execution
- resume recovery-required execution
- view live node transitions
- view final outcome and failure reason

### 5. First-Class Recovery

Recovery is not complete unless ADE can operate it. The product must expose:

- execution history with status and timestamps
- recovery-required indicator
- failure reason
- recovery action recommendation
- resume control where allowed
- clear non-resumable state where retry safety forbids resume

### 6. First-Class Observability

Workflow execution must emit typed workflow-specific events. Generic session events are insufficient. ADE needs:

- workflow execution started
- node started
- node completed
- node failed
- execution completed
- execution recovery required

Each event must include enough identifiers to update the correct workflow and node without inference.

## Design Principles

### P1. Contract before code

The first implementation step is to lock the shared workflow schema. No UI or runtime extension is allowed before the schema is explicit.

### P2. Runtime truth over aspirational UI

If a feature is not implemented in the runtime, remove it from the UI until it is.

### P3. Recovery is part of the main path

Execution history and resume are core product paths, not support tooling.

### P4. Status must be attributable

Every node transition and terminal workflow state must have a typed cause and a user-visible explanation path.

### P5. Prefer boring explicitness

Use explicit node kinds, explicit config fields, explicit event payloads, and explicit validation rules over generic untyped JSON.

## Required Architectural Decisions

This implementation package adopts the following decisions:

1. Workflow runtime configuration is canonical under `node.config`, not on ad hoc top-level runtime-only keys.
2. The gateway must normalize stored workflow JSON into typed structs before execution.
3. Advanced orchestration concepts such as explicit branching, parallel fork/join, sub-workflows, and A/B routing are follow-on work. They are out of scope for the truthful production cut unless implemented end to end first.
4. A workflow-specific event stream is required. Reusing generic `SessionEvent` is not sufficient.
5. Execution history needs a detail view and cannot remain list-only.
6. A workflow execution event log should exist if the current state snapshot alone cannot support detail and timeline UX cleanly.

## Required Product Semantics

### Supported node types in the target system

The target workflow system supports:

- `llm_call`
- `tool_exec`
- `gate_check`
- `transform`
- `condition`
- `wait`

The current UI type `parallel_branch` is deprecated and must be removed.

Advanced orchestration semantics are explicitly deferred:

- explicit branch routing
- parallel fork/join
- sub-workflow invocation
- A/B routing

### Execution status model

Workflow execution statuses:

- `queued`
- `running`
- `completed`
- `failed`
- `recovery_required`
- `cancelled` if cancellation is added later

Node execution statuses:

- `pending`
- `running`
- `completed`
- `failed`
- `skipped`
- `passed`

### Failure handling model

- Validation failures must fail before execution begins.
- Unknown node kinds must be impossible after schema validation.
- Retry-safe in-flight nodes may be resumed.
- Non-retry-safe in-flight nodes must move to `recovery_required` with an explicit reason.
- ADE must display failure reason and recommended next action.

## Delivery Definition

This project is complete only when all of the following are true:

- the editor only exposes real runtime features
- the runtime supports every visible feature
- the SDK exposes the full workflow lifecycle ADE needs
- ADE can list, edit, execute, inspect, and handle recovery truthfully
- node-level live status updates render correctly in ADE
- contract and integration tests fail on schema drift
- live-surface audit covers create, edit, execute, history, detail, and recovery flows

## Non-Goals for This Cut

The following are explicitly deferred unless later added as follow-on work:

- explicit branch-routing semantics
- parallel workflow execution
- sub-workflow invocation
- A/B experimentation nodes
- distributed workflow execution across multiple hosts
- arbitrary user-defined scripting inside nodes
- speculative execution
- versioned workflow diff/merge UX
- multi-user collaborative graph editing

## Exit Condition

The workflow tab may be called production-grade only when it is a single coherent system from graph authoring to live execution and recovery, with no dead features, no hidden contract forks, and no UI/runtime semantic mismatch.
