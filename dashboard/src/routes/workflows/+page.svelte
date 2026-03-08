<script lang="ts">
  /**
   * Workflow Composer page (T-2.6.1, T-2.6.3).
   * Visual DAG editor + save/load/execute.
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import WorkflowCanvas from '../../components/WorkflowCanvas.svelte';
  import type { WorkflowNode, WorkflowEdge } from '../../components/WorkflowCanvas.svelte';
  import WorkflowNodeConfig from '../../components/WorkflowNodeConfig.svelte';
  import type { ExecuteWorkflowResult, Workflow } from '@ghost/sdk';

  // Workflow list
  let workflows: Workflow[] = $state([]);
  let listLoading = $state(true);

  // Active workflow
  let activeWorkflow: Workflow | null = $state(null);
  let nodes: WorkflowNode[] = $state([]);
  let edges: WorkflowEdge[] = $state([]);
  let selectedNodeId: string | null = $state(null);
  let workflowName = $state('');
  let workflowDesc = $state('');

  // Actions
  let saving = $state(false);
  let executing = $state(false);
  // T-5.9.5: Typed execution result.
  let executeResult: ExecuteWorkflowResult | null = $state(null);
  let error = $state('');

  let canvas: WorkflowCanvas;

  let selectedNode = $derived(selectedNodeId ? nodes.find(n => n.id === selectedNodeId) ?? null : null);

  onMount(async () => {
    await loadWorkflows();
  });

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

  function newWorkflow() {
    activeWorkflow = null;
    nodes = [];
    edges = [];
    selectedNodeId = null;
    workflowName = 'New Workflow';
    workflowDesc = '';
    executeResult = null;
    error = '';
  }

  async function loadWorkflow(id: string) {
    try {
      const client = await getGhostClient();
      const data = await client.workflows.get(id);
      activeWorkflow = data;
      nodes = (data.nodes as WorkflowNode[]) ?? [];
      edges = (data.edges as WorkflowEdge[]) ?? [];
      workflowName = data.name;
      workflowDesc = data.description ?? '';
      selectedNodeId = null;
      executeResult = null;
      error = '';
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
    executeResult = null;
    error = '';
    try {
      const client = await getGhostClient();
      const data = await client.workflows.execute(activeWorkflow.id);
      executeResult = data;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Execution failed';
    }
    executing = false;
  }

  function handleNodeUpdate(updated: WorkflowNode) {
    nodes = nodes.map(n => n.id === updated.id ? updated : n);
  }

  function handleNodeDelete(nodeId: string) {
    canvas.removeNode(nodeId);
  }

  function formatWorkflowDate(updatedAt?: string): string {
    if (!updatedAt) return 'Unknown';
    return new Date(updatedAt).toLocaleDateString();
  }

  const NODE_TYPES = [
    { type: 'llm_call', label: 'LLM Call' },
    { type: 'tool_exec', label: 'Tool Exec' },
    { type: 'gate_check', label: 'Gate Check' },
    { type: 'transform', label: 'Transform' },
    { type: 'condition', label: 'Condition' },
    { type: 'parallel_branch', label: 'Parallel' },
    { type: 'sub_workflow', label: 'Sub-Workflow' },
    { type: 'ab_test', label: 'A/B Test' },
  ] as const;
</script>

<div class="composer-layout">
  <!-- Sidebar: Workflow List -->
  <aside class="wf-sidebar">
    <div class="sidebar-header">
      <h2>Workflows</h2>
      <button class="btn-new" onclick={newWorkflow}>+ New</button>
    </div>
    {#if listLoading}
      <div class="loading-sm">Loading…</div>
    {:else}
      <ul class="wf-list">
        {#each workflows as wf (wf.id)}
          <li>
            <button
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

  <!-- Main Canvas Area -->
  <div class="wf-main">
    <div class="toolbar">
      <input type="text" class="name-input" bind:value={workflowName} placeholder="Workflow name" />

      <div class="node-buttons">
        {#each NODE_TYPES as nt}
          <button class="btn-add-node" onclick={() => canvas.addNode(nt.type, nt.label)}>
            + {nt.label}
          </button>
        {/each}
      </div>

      <div class="action-buttons">
        <button class="btn-save" disabled={saving} onclick={saveWorkflow}>
          {saving ? 'Saving…' : 'Save'}
        </button>
        {#if activeWorkflow}
          <button class="btn-exec" disabled={executing} onclick={executeWorkflow}>
            {executing ? 'Running…' : 'Execute'}
          </button>
        {/if}
      </div>
    </div>

    <div class="hint">
      <span>Shift+click a node to connect. Drag to move. Scroll to zoom.</span>
    </div>

    <WorkflowCanvas bind:this={canvas} bind:nodes bind:edges bind:selectedNodeId onnodeselect={(id) => selectedNodeId = id} />

    {#if error}
      <div class="error-bar">{error}</div>
    {/if}

    {#if executeResult}
      <section class="result-card">
        <h3>Execution Result</h3>
        <pre class="result-json">{JSON.stringify(executeResult, null, 2)}</pre>
      </section>
    {/if}
  </div>

  <!-- Node Config Panel -->
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

  .sidebar-header h2 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
    margin: 0;
  }

  .btn-new {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-2);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .wf-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .wf-item {
    display: flex;
    flex-direction: column;
    width: 100%;
    text-align: left;
    padding: var(--spacing-2) var(--spacing-3);
    background: none;
    border: none;
    border-bottom: 1px solid var(--color-border-subtle);
    cursor: pointer;
    color: var(--color-text-primary);
  }

  .wf-item:hover { background: var(--color-bg-elevated-2); }
  .wf-item.active { background: color-mix(in srgb, var(--color-interactive-primary) 10%, transparent); }

  .wf-name { font-size: var(--font-size-sm); font-weight: var(--font-weight-medium); }
  .wf-date { font-size: var(--font-size-xs); color: var(--color-text-disabled); }

  .wf-main {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    padding: var(--spacing-3);
    overflow: hidden;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--spacing-3);
    flex-wrap: wrap;
  }

  .name-input {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    width: 200px;
  }

  .node-buttons {
    display: flex;
    gap: var(--spacing-1);
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

  .btn-save, .btn-exec {
    padding: var(--spacing-1) var(--spacing-3);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .btn-save {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
  }

  .btn-exec {
    background: var(--color-severity-normal);
    color: var(--color-text-inverse);
  }

  .btn-save:disabled, .btn-exec:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .hint {
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
  }

  .error-bar {
    padding: var(--spacing-2) var(--spacing-3);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
  }

  .result-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
  }

  .result-card h3 {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    color: var(--color-text-muted);
    margin: 0 0 var(--spacing-2) 0;
  }

  .result-json {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    max-height: 200px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .loading-sm {
    padding: var(--spacing-4);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  @media (max-width: 768px) {
    .wf-sidebar { display: none; }
    .node-buttons { display: none; }
  }
</style>
