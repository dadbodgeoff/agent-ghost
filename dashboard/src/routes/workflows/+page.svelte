<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import WorkflowCanvas from '../../components/WorkflowCanvas.svelte';
  import type { WorkflowEdge, WorkflowNode } from '../../components/WorkflowCanvas.svelte';
  import WorkflowNodeConfig from '../../components/WorkflowNodeConfig.svelte';
  import type {
    ExecuteWorkflowResult,
    Workflow,
    WorkflowExecutionSummary,
  } from '@ghost/sdk';

  let workflows: Workflow[] = $state([]);
  let listLoading = $state(true);
  let historyLoading = $state(false);

  let activeWorkflow: Workflow | null = $state(null);
  let nodes: WorkflowNode[] = $state([]);
  let edges: WorkflowEdge[] = $state([]);
  let selectedNodeId: string | null = $state(null);
  let workflowName = $state('');
  let workflowDesc = $state('');

  let saving = $state(false);
  let executing = $state(false);
  let resumeLoading = $state(false);
  let executeResult: ExecuteWorkflowResult | null = $state(null);
  let executionHistory: WorkflowExecutionSummary[] = $state([]);
  let selectedExecution: ExecuteWorkflowResult | null = $state(null);
  let activeExecutionId: string | null = $state(null);
  let error = $state('');

  let canvas: WorkflowCanvas;

  const NODE_TYPES = [
    { type: 'llm_call', label: 'LLM Call' },
    { type: 'tool_exec', label: 'Tool Exec' },
    { type: 'gate_check', label: 'Gate Check' },
    { type: 'transform', label: 'Transform' },
    { type: 'condition', label: 'Condition' },
    { type: 'wait', label: 'Wait' },
  ] as const;

  let selectedNode = $derived(
    selectedNodeId ? nodes.find((node) => node.id === selectedNodeId) ?? null : null,
  );

  onMount(() => {
    void loadWorkflows();

    const unsubscribers = [
      wsStore.on('WorkflowExecutionStarted', (msg) => {
        const workflowId = activeWorkflow?.id;
        if (!workflowId || msg.workflow_id !== workflowId) return;
        activeExecutionId = msg.execution_id as string;
        void loadExecutions(workflowId);
      }),
      wsStore.on('WorkflowExecutionResumed', (msg) => {
        const workflowId = activeWorkflow?.id;
        if (!workflowId || msg.workflow_id !== workflowId) return;
        activeExecutionId = msg.execution_id as string;
        void loadExecutionDetail(msg.execution_id as string);
      }),
      wsStore.on('WorkflowNodeStarted', (msg) => {
        if (!matchesActiveExecution(msg)) return;
        const nodeId = msg.node_id as string;
        nodes = nodes.map((node) =>
          node.id === nodeId ? { ...node, execution_status: 'running' } : node,
        );
      }),
      wsStore.on('WorkflowNodeCompleted', (msg) => {
        if (!matchesActiveExecution(msg)) return;
        const nodeId = msg.node_id as string;
        const status = mapExecutionStatus(msg.status as string | undefined);
        nodes = nodes.map((node) =>
          node.id === nodeId ? { ...node, execution_status: status } : node,
        );
      }),
      wsStore.on('WorkflowNodeFailed', (msg) => {
        if (!matchesActiveExecution(msg)) return;
        const nodeId = msg.node_id as string;
        nodes = nodes.map((node) =>
          node.id === nodeId ? { ...node, execution_status: 'failed' } : node,
        );
      }),
      wsStore.on('WorkflowExecutionCompleted', (msg) => {
        const workflowId = activeWorkflow?.id;
        if (!workflowId || msg.workflow_id !== workflowId) return;
        activeExecutionId = msg.execution_id as string;
        void refreshExecutionSurfaces(msg.execution_id as string);
      }),
      wsStore.on('WorkflowExecutionRecoveryRequired', (msg) => {
        const workflowId = activeWorkflow?.id;
        if (!workflowId || msg.workflow_id !== workflowId) return;
        activeExecutionId = msg.execution_id as string;
        void refreshExecutionSurfaces(msg.execution_id as string);
      }),
    ];

    return () => {
      for (const unsubscribe of unsubscribers) unsubscribe();
    };
  });

  function matchesActiveExecution(msg: Record<string, unknown>): boolean {
    return (
      msg.workflow_id === activeWorkflow?.id &&
      typeof msg.execution_id === 'string' &&
      msg.execution_id === activeExecutionId
    );
  }

  function normalizeNode(rawNode: Record<string, unknown>, index: number): WorkflowNode {
    const type = typeof rawNode.type === 'string' ? rawNode.type : 'transform';
    const label =
      typeof rawNode.label === 'string' && rawNode.label.trim().length > 0
        ? rawNode.label
        : NODE_TYPES.find((entry) => entry.type === type)?.label ?? type;

    return {
      id:
        typeof rawNode.id === 'string' && rawNode.id.trim().length > 0
          ? rawNode.id
          : `node-${index + 1}`,
      type,
      label,
      x: typeof rawNode.x === 'number' ? rawNode.x : 80 + index * 40,
      y: typeof rawNode.y === 'number' ? rawNode.y : 80 + index * 24,
      config:
        rawNode.config && typeof rawNode.config === 'object' && !Array.isArray(rawNode.config)
          ? { ...(rawNode.config as Record<string, unknown>) }
          : {},
      execution_status: undefined,
    };
  }

  function normalizeEdge(rawEdge: Record<string, unknown>): WorkflowEdge {
    return {
      source: String(rawEdge.source ?? ''),
      target: String(rawEdge.target ?? ''),
      condition_label:
        typeof rawEdge.condition_label === 'string' ? rawEdge.condition_label : undefined,
      branch_type:
        typeof rawEdge.branch_type === 'string'
          ? (rawEdge.branch_type as WorkflowEdge['branch_type'])
          : undefined,
    };
  }

  function normalizeExecutionResult(
    execution: ExecuteWorkflowResult | null | undefined,
  ): ExecuteWorkflowResult | null {
    if (!execution) return null;
    return {
      ...execution,
      mode: execution.mode ?? 'dag',
      steps: Array.isArray(execution.steps) ? execution.steps : [],
      recovery_required: execution.recovery_required ?? false,
    };
  }

  function resetExecutionStatuses() {
    nodes = nodes.map((node) => ({ ...node, execution_status: undefined }));
  }

  function clearExecutionState() {
    executeResult = null;
    selectedExecution = null;
    executionHistory = [];
    activeExecutionId = null;
    resetExecutionStatuses();
  }

  function applyExecutionToCanvas(execution: ExecuteWorkflowResult | null) {
    resetExecutionStatuses();
    if (!execution) return;

    const statusByNode = new Map<string, WorkflowNode['execution_status']>();
    for (const step of execution.steps ?? []) {
      statusByNode.set(step.node_id, mapExecutionStatus(step.result?.status));
    }
    if (typeof execution.current_node_id === 'string' && execution.status === 'running') {
      statusByNode.set(execution.current_node_id, 'running');
    }

    nodes = nodes.map((node) => ({
      ...node,
      execution_status: statusByNode.get(node.id),
    }));
  }

  function mapExecutionStatus(status?: string): WorkflowNode['execution_status'] {
    switch (status) {
      case 'running':
        return 'running';
      case 'completed':
      case 'passed':
        return 'completed';
      case 'skipped':
        return 'skipped';
      case 'failed':
        return 'failed';
      default:
        return undefined;
    }
  }

  async function loadWorkflows() {
    try {
      listLoading = true;
      const client = await getGhostClient();
      const data = await client.workflows.list({ limit: 50 });
      workflows = data?.workflows ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load workflows';
    }
    listLoading = false;
  }

  async function loadExecutions(workflowId: string) {
    try {
      historyLoading = true;
      const client = await getGhostClient();
      const data = await client.workflows.listExecutions(workflowId);
      executionHistory = data.executions ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load execution history';
    }
    historyLoading = false;
  }

  async function loadExecutionDetail(executionId: string) {
    if (!activeWorkflow) return;
    try {
      const client = await getGhostClient();
      const data = normalizeExecutionResult(
        await client.workflows.getExecution(activeWorkflow.id, executionId),
      );
      if (!data) return;
      selectedExecution = data;
      executeResult = data;
      activeExecutionId = data.execution_id;
      applyExecutionToCanvas(data);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load execution detail';
    }
  }

  async function refreshExecutionSurfaces(executionId: string) {
    if (!activeWorkflow) return;
    await loadExecutions(activeWorkflow.id);
    await loadExecutionDetail(executionId);
  }

  function newWorkflow() {
    activeWorkflow = null;
    nodes = [];
    edges = [];
    selectedNodeId = null;
    workflowName = 'New Workflow';
    workflowDesc = '';
    error = '';
    clearExecutionState();
  }

  async function loadWorkflow(id: string) {
    try {
      const client = await getGhostClient();
      const data = await client.workflows.get(id);
      activeWorkflow = data;
      nodes = (data.nodes as unknown[]).map((node, index) =>
        normalizeNode((node ?? {}) as Record<string, unknown>, index),
      );
      edges = (data.edges as unknown[]).map((edge) =>
        normalizeEdge((edge ?? {}) as Record<string, unknown>),
      );
      workflowName = data.name;
      workflowDesc = data.description ?? '';
      selectedNodeId = null;
      error = '';
      clearExecutionState();
      await loadExecutions(id);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Operation failed';
    }
  }

  async function saveWorkflow() {
    saving = true;
    error = '';
    try {
      const payload = {
        name: workflowName,
        description: workflowDesc,
        nodes,
        edges,
      };

      const client = await getGhostClient();
      if (activeWorkflow) {
        await client.workflows.update(activeWorkflow.id, payload);
        activeWorkflow = { ...activeWorkflow, name: workflowName, description: workflowDesc, nodes, edges };
        await loadExecutions(activeWorkflow.id);
      } else {
        const data = await client.workflows.create(payload);
        activeWorkflow = {
          id: data.id,
          name: data.name,
          description: data.description,
          nodes,
          edges,
        };
      }
      await loadWorkflows();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to save workflow';
    }
    saving = false;
  }

  async function executeWorkflow() {
    if (!activeWorkflow) return;
    executing = true;
    error = '';
    clearExecutionState();
    try {
      const client = await getGhostClient();
      const data = normalizeExecutionResult(await client.workflows.execute(activeWorkflow.id));
      if (!data) return;
      executeResult = data;
      selectedExecution = data;
      activeExecutionId = data.execution_id;
      applyExecutionToCanvas(data);
      await loadExecutions(activeWorkflow.id);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Execution failed';
    }
    executing = false;
  }

  async function resumeExecution() {
    if (!activeWorkflow || !selectedExecution) return;
    resumeLoading = true;
    error = '';
    try {
      const client = await getGhostClient();
      const data = normalizeExecutionResult(
        await client.workflows.resume(activeWorkflow.id, selectedExecution.execution_id),
      );
      if (!data) return;
      executeResult = data;
      selectedExecution = data;
      activeExecutionId = data.execution_id;
      applyExecutionToCanvas(data);
      await loadExecutions(activeWorkflow.id);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Resume failed';
    }
    resumeLoading = false;
  }

  function handleNodeUpdate(updated: WorkflowNode) {
    nodes = nodes.map((node) => (node.id === updated.id ? updated : node));
  }

  function handleNodeDelete(nodeId: string) {
    canvas.removeNode(nodeId);
  }

  function formatWorkflowDate(updatedAt?: string): string {
    if (!updatedAt) return 'Unknown';
    return new Date(updatedAt).toLocaleDateString();
  }

  function formatExecutionTime(timestamp?: string | null): string {
    if (!timestamp) return 'Unknown';
    return new Date(timestamp).toLocaleString();
  }

  function isExecutionResumable(execution: ExecuteWorkflowResult | null): boolean {
    if (!execution?.recovery_required) return false;
    return (execution.recovery_action ?? '').startsWith('resume_');
  }

  function executionStatusTone(status: string): string {
    switch (status) {
      case 'completed':
      case 'passed':
        return 'tone-good';
      case 'running':
        return 'tone-running';
      case 'recovery_required':
      case 'skipped':
        return 'tone-warning';
      default:
        return 'tone-bad';
    }
  }
</script>

<div class="composer-layout">
  <aside class="wf-sidebar">
    <div class="sidebar-header">
      <h2>Workflows</h2>
      <button type="button" class="btn-new" onclick={newWorkflow}>+ New</button>
    </div>
    {#if listLoading}
      <div class="loading-sm">Loading…</div>
    {:else}
      <ul class="wf-list">
        {#each workflows as wf (wf.id)}
          <li>
            <button
              type="button"
              class="wf-item"
              class:active={activeWorkflow?.id === wf.id}
              onclick={() => loadWorkflow(wf.id)}
            >
              <span class="wf-name">{wf.name}</span>
              <span class="wf-date">{formatWorkflowDate(wf.updated_at)}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </aside>

  <div class="wf-main">
    <div class="toolbar">
      <div class="title-group">
        <input type="text" class="name-input" bind:value={workflowName} placeholder="Workflow name" />
        <textarea
          class="desc-input"
          bind:value={workflowDesc}
          rows="2"
          placeholder="Workflow description"
        ></textarea>
      </div>

      <div class="node-buttons">
        {#each NODE_TYPES as nt}
          <button type="button" class="btn-add-node" onclick={() => canvas.addNode(nt.type, nt.label)}>
            + {nt.label}
          </button>
        {/each}
      </div>

      <div class="action-buttons">
        <button type="button" class="btn-save" disabled={saving} onclick={saveWorkflow}>
          {saving ? 'Saving…' : 'Save'}
        </button>
        {#if activeWorkflow}
          <button type="button" class="btn-exec" disabled={executing} onclick={executeWorkflow}>
            {executing ? 'Running…' : 'Execute'}
          </button>
        {/if}
      </div>
    </div>

    <div class="hint">
      <span>Supported runtime nodes: LLM Call, Tool Exec, Gate Check, Transform, Condition, Wait.</span>
      <span>Shift+click a node to connect. Drag to move. Scroll to zoom.</span>
    </div>

    <WorkflowCanvas
      bind:this={canvas}
      bind:nodes
      bind:edges
      bind:selectedNodeId
      onnodeselect={(id) => selectedNodeId = id}
    />

    {#if error}
      <div class="error-bar">{error}</div>
    {/if}

    {#if activeWorkflow}
      <section class="execution-shell">
        <div class="history-column">
          <div class="section-head">
            <h3>Execution History</h3>
            {#if historyLoading}<span class="section-meta">Refreshing…</span>{/if}
          </div>
          {#if executionHistory.length === 0}
            <div class="empty-card">No executions yet.</div>
          {:else}
            <ul class="execution-list">
              {#each executionHistory as execution (execution.execution_id)}
                <li>
                  <button
                    type="button"
                    class="execution-item"
                    class:active={selectedExecution?.execution_id === execution.execution_id}
                    onclick={() => loadExecutionDetail(execution.execution_id)}
                  >
                    <span class={`execution-status ${executionStatusTone(execution.status)}`}>
                      {execution.status}
                    </span>
                    <span class="execution-id mono">{execution.execution_id.slice(0, 12)}…</span>
                    <span class="execution-time">{formatExecutionTime(execution.started_at)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>

        <div class="detail-column">
          <div class="section-head">
            <h3>Execution Detail</h3>
            {#if isExecutionResumable(selectedExecution)}
              <button type="button" class="btn-resume" disabled={resumeLoading} onclick={resumeExecution}>
                {resumeLoading ? 'Resuming…' : 'Resume'}
              </button>
            {/if}
          </div>

          {#if selectedExecution}
            <div class="detail-card">
              <div class="detail-row">
                <span class="detail-label">Status</span>
                <span class={`execution-status ${executionStatusTone(selectedExecution.status)}`}>
                  {selectedExecution.status}
                </span>
              </div>
              <div class="detail-row">
                <span class="detail-label">Execution ID</span>
                <span class="mono">{selectedExecution.execution_id}</span>
              </div>
              <div class="detail-row">
                <span class="detail-label">Started</span>
                <span>{formatExecutionTime(selectedExecution.started_at)}</span>
              </div>
              <div class="detail-row">
                <span class="detail-label">Completed</span>
                <span>{formatExecutionTime(selectedExecution.completed_at)}</span>
              </div>
              {#if selectedExecution.recovery_action}
                <div class="detail-row">
                  <span class="detail-label">Recovery Action</span>
                  <span>{selectedExecution.recovery_action}</span>
                </div>
              {/if}
              {#if selectedExecution.reason}
                <div class="detail-row detail-row-stack">
                  <span class="detail-label">Reason</span>
                  <span>{selectedExecution.reason}</span>
                </div>
              {/if}
              {#if selectedExecution.recovery_required && !isExecutionResumable(selectedExecution)}
                <div class="detail-row detail-row-stack">
                  <span class="detail-label">Recovery</span>
                  <span>Manual recovery required. No safe automatic resume path is exposed for this execution.</span>
                </div>
              {/if}
            </div>

            <div class="steps-card">
              <h4>Node Timeline</h4>
              {#if selectedExecution.steps.length === 0}
                <div class="empty-card">No node activity recorded yet.</div>
              {:else}
                <ol class="step-list">
                  {#each selectedExecution.steps as step}
                    <li class="step-item">
                      <div class="step-head">
                        <span class="mono">{step.node_id}</span>
                        <span class={`execution-status ${executionStatusTone(step.result?.status ?? 'failed')}`}>
                          {step.result?.status ?? 'unknown'}
                        </span>
                      </div>
                      <div class="step-meta">
                        <span>{step.node_type}</span>
                        <span>{formatExecutionTime(step.started_at)}</span>
                      </div>
                    </li>
                  {/each}
                </ol>
              {/if}
            </div>
          {:else}
            <div class="empty-card">Select an execution to inspect node-level detail.</div>
          {/if}
        </div>
      </section>
    {/if}
  </div>

  <WorkflowNodeConfig
    node={selectedNode}
    onupdate={handleNodeUpdate}
    ondelete={handleNodeDelete}
    onclose={() => selectedNodeId = null}
  />
</div>

<style>
  .composer-layout {
    display: flex;
    gap: 0;
    min-height: calc(100vh - 80px);
  }

  .wf-sidebar {
    width: 220px;
    border-right: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
    overflow-y: auto;
    flex-shrink: 0;
  }

  .sidebar-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-3);
    border-bottom: 1px solid var(--color-border-default);
  }

  .sidebar-header h2,
  .section-head h3,
  .steps-card h4 {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
  }

  .btn-new,
  .btn-save,
  .btn-exec,
  .btn-resume {
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-weight: var(--font-weight-medium);
  }

  .btn-new {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    padding: var(--spacing-1) var(--spacing-2);
    font-size: var(--font-size-xs);
  }

  .wf-list,
  .execution-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .wf-item,
  .execution-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
    text-align: left;
    padding: var(--spacing-2) var(--spacing-3);
    background: none;
    border: none;
    border-bottom: 1px solid var(--color-border-subtle);
    cursor: pointer;
    color: var(--color-text-primary);
  }

  .wf-item:hover,
  .execution-item:hover {
    background: var(--color-bg-elevated-2);
  }

  .wf-item.active,
  .execution-item.active {
    background: color-mix(in srgb, var(--color-interactive-primary) 10%, transparent);
  }

  .wf-name {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
  }

  .wf-date,
  .execution-time,
  .section-meta,
  .step-meta {
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
  }

  .wf-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    padding: var(--spacing-3);
    overflow: auto;
  }

  .toolbar {
    display: flex;
    align-items: flex-start;
    gap: var(--spacing-3);
    flex-wrap: wrap;
  }

  .title-group {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    min-width: 260px;
  }

  .name-input,
  .desc-input {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    padding: var(--spacing-2);
  }

  .desc-input {
    resize: vertical;
    min-height: 58px;
  }

  .node-buttons {
    display: flex;
    gap: var(--spacing-1);
    flex-wrap: wrap;
  }

  .btn-add-node {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-2);
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .btn-add-node:hover {
    border-color: var(--color-interactive-primary);
    color: var(--color-interactive-primary);
  }

  .action-buttons {
    display: flex;
    gap: var(--spacing-2);
    margin-left: auto;
  }

  .btn-save,
  .btn-exec,
  .btn-resume {
    padding: var(--spacing-2) var(--spacing-3);
    font-size: var(--font-size-sm);
  }

  .btn-save {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
  }

  .btn-exec,
  .btn-resume {
    background: var(--color-severity-normal);
    color: var(--color-text-inverse);
  }

  .btn-save:disabled,
  .btn-exec:disabled,
  .btn-resume:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .hint {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-3);
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
  }

  .error-bar,
  .detail-card,
  .steps-card,
  .empty-card {
    border-radius: var(--radius-sm);
    padding: var(--spacing-3);
  }

  .error-bar {
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border: 1px solid var(--color-severity-hard);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
  }

  .execution-shell {
    display: grid;
    grid-template-columns: minmax(240px, 300px) minmax(0, 1fr);
    gap: var(--spacing-3);
  }

  .history-column,
  .detail-column {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    min-width: 0;
  }

  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .detail-card,
  .steps-card,
  .empty-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-3);
    padding: var(--spacing-1) 0;
    border-bottom: 1px solid var(--color-border-subtle);
    font-size: var(--font-size-sm);
  }

  .detail-row:last-child {
    border-bottom: none;
  }

  .detail-row-stack {
    flex-direction: column;
  }

  .detail-label {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .execution-status {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: fit-content;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: var(--font-size-xs);
    text-transform: uppercase;
  }

  .tone-good {
    background: color-mix(in srgb, var(--color-score-high) 14%, transparent);
    color: var(--color-score-high);
  }

  .tone-running {
    background: color-mix(in srgb, var(--color-severity-active) 14%, transparent);
    color: var(--color-severity-active);
  }

  .tone-warning {
    background: color-mix(in srgb, var(--color-severity-normal) 14%, transparent);
    color: var(--color-severity-normal);
  }

  .tone-bad {
    background: color-mix(in srgb, var(--color-severity-hard) 14%, transparent);
    color: var(--color-severity-hard);
  }

  .step-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    margin: var(--spacing-2) 0 0 0;
    padding-left: var(--spacing-4);
  }

  .step-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .step-head {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-2);
    align-items: center;
  }

  .mono {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .loading-sm {
    padding: var(--spacing-4);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  @media (max-width: 1100px) {
    .execution-shell {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 768px) {
    .wf-sidebar {
      display: none;
    }

    .node-buttons {
      display: none;
    }

    .action-buttons {
      margin-left: 0;
    }
  }
</style>
