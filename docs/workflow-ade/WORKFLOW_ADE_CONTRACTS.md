# Workflow ADE Contracts

Status: March 11, 2026

Purpose: define the canonical data and event contracts for workflow authoring, persistence, execution, recovery, and live status.

This document exists to eliminate the current split between dashboard JSON shape and executor expectations.

## Contract Rules

- Dashboard, SDK, and gateway must all consume the same shape.
- Runtime must execute normalized typed structs, not raw unvalidated JSON.
- `node.config` is the canonical home for runtime configuration.
- Display-only fields must be explicitly separated from runtime fields.

## Workflow Definition

```ts
type WorkflowDefinition = {
  id: string;
  name: string;
  description: string;
  version: number;
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
  created_by: string | null;
  created_at: string;
  updated_at: string;
};
```

Rules:

- timestamps must be RFC 3339 UTC
- `version` increments on every persisted mutation
- workflow IDs are stable
- workflow definitions are immutable per execution once a run starts; executions snapshot the exact graph version they ran

## Workflow Node

```ts
type WorkflowNode =
  | LlmCallNode
  | ToolExecNode
  | GateCheckNode
  | TransformNode
  | ConditionNode
  | WaitNode;

type WorkflowNodeBase = {
  id: string;
  type: WorkflowNodeType;
  label: string;
  position: {
    x: number;
    y: number;
  };
  config: Record<string, unknown>;
  ui?: {
    color?: string;
    collapsed?: boolean;
  };
};
```

Rules:

- `position` is purely visual
- `config` is runtime-relevant
- no runtime-critical field may exist outside `config`
- no executor code may read `prompt`, `agent_id`, `wait_ms`, or similar from top-level node keys

## Supported Node Kinds

### `llm_call`

```ts
type LlmCallNode = WorkflowNodeBase & {
  type: "llm_call";
  config: {
    agent_id?: string;
    system_prompt?: string;
    model_profile?: string;
    input_template?: string;
    output_key?: string;
  };
};
```

Semantics:

- consumes upstream input
- invokes the runtime model path
- emits structured output and text summary

### `tool_exec`

```ts
type ToolExecNode = WorkflowNodeBase & {
  type: "tool_exec";
  config: {
    skill_name: string;
    agent_id?: string;
  };
};
```

Semantics:

- resolves a real skill/tool binding
- executes through the governed skill catalog path
- emits the tool result payload as node output

### `gate_check`

```ts
type GateCheckNode = WorkflowNodeBase & {
  type: "gate_check";
  config: {
    gate_name?: string;
  };
};
```

Semantics:

- current production cut treats this as a pass-through checkpoint
- the node reports `passed` and forwards input unchanged
- no thresholding or alternate routing is active in this cut

### `transform`

```ts
type TransformNode = WorkflowNodeBase & {
  type: "transform";
  config: {
    transform_name?: string;
    params?: Record<string, unknown>;
  };
};
```

Semantics:

- current production cut is a deterministic pass-through node
- future named transforms must not be added until runtime semantics exist

### `condition`

```ts
type ConditionNode = WorkflowNodeBase & {
  type: "condition";
  config: {
    expression: string;
  };
};
```

Semantics:

- evaluates a simple expression against current input
- current production cut treats this as a linear predicate, not a branch router
- `true`, an exact string match against serialized input, or any non-null input passes
- failed conditions stop the workflow at that node

### `wait`

```ts
type WaitNode = WorkflowNodeBase & {
  type: "wait";
  config: {
    wait_ms: number;
  };
};
```

Semantics:

- delays execution for bounded duration

## Workflow Edge

```ts
type WorkflowEdge = {
  source: string;
  target: string;
  condition_label?: string;
  branch_type?: "parallel" | "conditional" | "default" | string;
};
```

Rules:

- edges are runtime-significant as dependency ordering
- in the current production cut, edge metadata is visual only and is not interpreted by the executor
- any future routing semantics require a contract revision before ADE can expose them
- `source` and `target` must reference known node IDs

## Persisted Execution State

```ts
type WorkflowExecution = {
  execution_id: string;
  workflow_id: string;
  workflow_version: number;
  workflow_name: string;
  status: WorkflowExecutionStatus;
  input: unknown;
  output?: unknown;
  started_at: string;
  completed_at?: string;
  current_node_id?: string;
  current_step_index?: number;
  recovery_required: boolean;
  recovery_action?: string;
  recovery_reason?: string;
  graph_snapshot: {
    nodes: WorkflowNode[];
    edges: WorkflowEdge[];
  };
  node_states: Record<string, WorkflowNodeState>;
  metadata?: Record<string, unknown>;
};
```

```ts
type WorkflowNodeState = {
  node_id: string;
  status: "pending" | "running" | "completed" | "failed" | "skipped" | "passed";
  attempt: number;
  started_at?: string;
  completed_at?: string;
  input?: unknown;
  output?: unknown;
  error?: {
    code: string;
    message: string;
    retry_safe: boolean;
  };
};
```

Rules:

- execution rows must snapshot the graph they ran
- execution rows must remain readable even if the workflow definition later changes
- terminal response bodies may not be the only place execution detail exists

## Execution Event Contract

The system should persist a workflow execution event log.

```ts
type WorkflowExecutionEvent = {
  event_id: string;
  execution_id: string;
  workflow_id: string;
  node_id?: string;
  type:
    | "workflow_execution_started"
    | "workflow_node_started"
    | "workflow_node_completed"
    | "workflow_node_failed"
    | "workflow_execution_completed"
    | "workflow_execution_failed"
    | "workflow_execution_recovery_required"
    | "workflow_execution_resumed";
  timestamp: string;
  payload: Record<string, unknown>;
};
```

Why this is required:

- ADE needs timeline detail
- WebSocket reconnect/resync should have a durable source of truth
- recovery diagnosis should not depend only on the final snapshot

## REST Contract

Required endpoints:

- `GET /api/workflows`
- `GET /api/workflows/:id`
- `POST /api/workflows`
- `PUT /api/workflows/:id`
- `POST /api/workflows/:id/validate`
- `POST /api/workflows/:id/execute`
- `GET /api/workflows/:id/executions`
- `GET /api/workflows/:id/executions/:execution_id`
- `POST /api/workflows/:id/resume/:execution_id`

Optional later:

- `POST /api/workflows/:id/cancel/:execution_id`

Rules:

- all mutation routes must remain idempotent-aware where already required by gateway conventions
- validation route must return structured errors ADE can render inline
- execution detail route must return full normalized execution state

## WebSocket Contract

Workflow ADE requires workflow-specific events.

```ts
type WorkflowWsEvent =
  | {
      type: "WorkflowExecutionStarted";
      workflow_id: string;
      execution_id: string;
      workflow_version: number;
      started_at: string;
    }
  | {
      type: "WorkflowNodeStarted";
      workflow_id: string;
      execution_id: string;
      node_id: string;
      node_type: WorkflowNodeType;
      started_at: string;
    }
  | {
      type: "WorkflowNodeCompleted";
      workflow_id: string;
      execution_id: string;
      node_id: string;
      node_type: WorkflowNodeType;
      completed_at: string;
      status: "completed" | "skipped" | "passed";
    }
  | {
      type: "WorkflowNodeFailed";
      workflow_id: string;
      execution_id: string;
      node_id: string;
      node_type: WorkflowNodeType;
      completed_at: string;
      error_code: string;
      message: string;
      retry_safe: boolean;
    }
  | {
      type: "WorkflowExecutionCompleted";
      workflow_id: string;
      execution_id: string;
      completed_at: string;
      status: "completed" | "failed";
    }
  | {
      type: "WorkflowExecutionRecoveryRequired";
      workflow_id: string;
      execution_id: string;
      recovery_action: string;
      reason: string;
      occurred_at: string;
    };
```

Rules:

- events must include `workflow_id` and `execution_id`
- node events must include `node_id`
- ADE must not infer node identity from a generic session ID

## SDK Contract

The SDK must expose:

- list workflows
- get workflow
- create workflow
- update workflow
- execute workflow
- list executions
- get execution detail
- resume execution
- typed workflow WebSocket events

The SDK may not hand-roll workflow types that fork from generated types without an explicit exception record.

## Migration Contract

Because the current data model already stores workflow JSON, the implementation must handle legacy rows.

Legacy normalization rules:

- `parallel_branch` must be rejected or migrated explicitly; silent reinterpretation is forbidden
- top-level `prompt`, `agent_id`, `wait_ms`, and similar fields must be migrated into `config`
- workflows containing unknown node kinds must be marked invalid until migrated

## Contract Exit Criteria

This contract is complete only when:

- OpenAPI exposes these shapes explicitly
- generated SDK types match them
- hand-written SDK wrappers do not fork from them
- dashboard consumes only the shared workflow types
- runtime executes only normalized workflow structs derived from them
