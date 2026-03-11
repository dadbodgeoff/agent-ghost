# Workflow ADE Validation and Rollout

Status: March 11, 2026

Purpose: define the test doctrine, release gates, rollout order, and rollback conditions for the workflow ADE rebuild.

## Validation Doctrine

The workflow system is not validated by route presence or happy-path execution alone.

It is validated only when all four layers agree:

- contract layer
- backend/runtime layer
- SDK/client layer
- ADE/operator layer

## Required Test Layers

### 1. Contract tests

Must prove:

- OpenAPI workflow schemas match backend request/response structs
- generated SDK types match OpenAPI
- hand-written SDK wrappers do not fork from generated types
- dashboard workflow types are imported from shared SDK or generated types where possible

### 2. Backend tests

Must cover:

- workflow create/update validation
- legacy workflow normalization or rejection
- every supported node kind
- current condition-node predicate semantics
- retry-safe resume
- non-retry-safe recovery-required failure
- execution detail retrieval
- workflow-specific WebSocket event payloads

### 3. SDK tests

Must cover:

- all workflow API methods
- execution detail typing
- resume typing
- workflow WebSocket event typing

### 4. Dashboard tests

Must cover:

- authoring a valid workflow
- invalid graph feedback
- execution history rendering
- execution detail rendering
- node-level live status updates
- resume flow only for recovery-required executions that expose a machine-resumable recovery action
- feature visibility matching supported node kinds

### 5. Live-surface audit

The live audit must prove:

- ADE can create a workflow
- ADE can edit a workflow
- ADE can execute a workflow
- ADE can load execution history
- ADE can load execution detail
- ADE can observe live node transitions
- ADE can only surface a resume control when the recovery action is machine-resumable

## Required Release Gates

The workflow rebuild may not ship unless all gates pass:

1. Contract parity gate
2. Gateway workflow integration test suite
3. SDK test suite
4. Dashboard E2E workflow suite
5. Live-surface workflow audit

No workflow-related gate may be marked informational only.

## Observability Requirements

Before release, the system must expose:

- workflow execution count
- workflow execution failure count
- recovery-required count
- average node latency by node kind
- workflow WebSocket event emission count
- workflow resume success/failure count

## Rollout Order

1. Land contract and storage changes.
2. Land runtime completion behind internal validation if needed.
3. Land SDK completion.
4. Land ADE completion.
5. Run full validation.
6. Enable the upgraded workflow UI without legacy dead features.

## Rollback Conditions

Rollback is required if any of the following occur:

- ADE exposes a node type the runtime rejects
- execution detail cannot explain a failure
- resume causes duplicate non-idempotent node side effects
- WebSocket workflow events cannot be correlated to node state in ADE
- live audit fails on create, execute, detail, or resume

## Manual QA Checklist

Manual verification must include:

- create a workflow using each supported node kind
- save and reload the workflow
- execute a successful linear workflow
- execute a conditional workflow and confirm pass/fail behavior
- force a retry-safe interruption and resume it
- force a non-retry-safe interruption and confirm `recovery_required`
- verify ADE live node transitions during execution

## Exit Condition

The rollout is complete only when the workflow surface is both product-complete and mechanically defended against regression.
