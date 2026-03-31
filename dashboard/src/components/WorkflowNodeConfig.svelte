<script lang="ts">
  import type { WorkflowNode } from './WorkflowCanvas.svelte';

  let {
    node,
    onupdate,
    ondelete,
    onclose,
  }: {
    node: WorkflowNode | null;
    onupdate?: (node: WorkflowNode) => void;
    ondelete?: (nodeId: string) => void;
    onclose?: () => void;
  } = $props();

  const NODE_TYPES = ['llm_call', 'tool_exec', 'gate_check', 'transform', 'condition', 'wait'] as const;

  function updateNode(patch: Partial<WorkflowNode>) {
    if (!node) return;
    onupdate?.({ ...node, ...patch });
  }

  function updateConfig(key: string, value: unknown) {
    if (!node) return;
    onupdate?.({ ...node, config: { ...node.config, [key]: value } });
  }

  function handleLabelChange(e: Event) {
    const target = e.target as HTMLInputElement;
    updateNode({ label: target.value });
  }

  function handleTypeChange(e: Event) {
    const target = e.target as HTMLSelectElement;
    updateNode({ type: target.value, config: {} });
  }

  function handleConfigChange(e: Event) {
    if (!node) return;
    const target = e.target as HTMLTextAreaElement;
    try {
      const config = JSON.parse(target.value);
      if (config && typeof config === 'object' && !Array.isArray(config)) {
        onupdate?.({ ...node, config });
      }
    } catch {
      // Ignore invalid JSON until the user corrects it.
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose?.();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if node}
  <div class="config-panel" role="dialog" aria-modal="true" aria-label="Node configuration">
    <div class="panel-header">
      <h3>Node Config</h3>
      <button class="close-btn" onclick={onclose} aria-label="Close panel">✕</button>
    </div>

    <div class="panel-body">
      <label class="field">
        <span class="field-label">Label</span>
        <input type="text" value={node.label} onchange={handleLabelChange} class="field-input" />
      </label>

      <label class="field">
        <span class="field-label">Type</span>
        <select value={node.type} onchange={handleTypeChange} class="field-input">
          {#each NODE_TYPES as type}
            <option value={type}>{type}</option>
          {/each}
        </select>
      </label>

      <label class="field">
        <span class="field-label">ID</span>
        <input type="text" value={node.id} readonly class="field-input mono" />
      </label>

      <div class="field">
        <span class="field-label">Position</span>
        <span class="field-value">x: {Math.round(node.x)}, y: {Math.round(node.y)}</span>
      </div>

      {#if node.type === 'llm_call'}
        <label class="field">
          <span class="field-label">Agent ID</span>
          <input
            type="text"
            value={String(node.config?.agent_id ?? '')}
            onchange={(e) => updateConfig('agent_id', (e.target as HTMLInputElement).value)}
            class="field-input mono"
            placeholder="Optional agent id"
          />
        </label>
        <label class="field">
          <span class="field-label">System Prompt</span>
          <textarea
            class="field-textarea"
            rows="4"
            onchange={(e) => updateConfig('system_prompt', (e.target as HTMLTextAreaElement).value)}
          >{String(node.config?.system_prompt ?? '')}</textarea>
        </label>
      {/if}

      {#if node.type === 'tool_exec'}
        <label class="field">
          <span class="field-label">Skill Name</span>
          <input
            type="text"
            value={String(node.config?.skill_name ?? node.config?.tool_name ?? '')}
            onchange={(e) => updateConfig('skill_name', (e.target as HTMLInputElement).value)}
            class="field-input mono"
            placeholder="e.g. file_read"
          />
        </label>
        <label class="field">
          <span class="field-label">Agent ID</span>
          <input
            type="text"
            value={String(node.config?.agent_id ?? '')}
            onchange={(e) => updateConfig('agent_id', (e.target as HTMLInputElement).value)}
            class="field-input mono"
            placeholder="Optional agent id"
          />
        </label>
      {/if}

      {#if node.type === 'gate_check'}
        <label class="field">
          <span class="field-label">Gate Name</span>
          <input
            type="text"
            value={String(node.config?.gate_name ?? '')}
            onchange={(e) => updateConfig('gate_name', (e.target as HTMLInputElement).value)}
            class="field-input mono"
            placeholder="Descriptive gate name"
          />
        </label>
      {/if}

      {#if node.type === 'condition'}
        <label class="field">
          <span class="field-label">Expression</span>
          <input
            type="text"
            value={String(node.config?.expression ?? node.condition ?? '')}
            onchange={(e) => updateConfig('expression', (e.target as HTMLInputElement).value)}
            class="field-input mono"
            placeholder="true or a string to match"
          />
        </label>
      {/if}

      {#if node.type === 'wait'}
        <label class="field">
          <span class="field-label">Wait (ms)</span>
          <input
            type="number"
            min="0"
            max="30000"
            value={String(node.config?.wait_ms ?? 1000)}
            onchange={(e) => updateConfig('wait_ms', Number.parseInt((e.target as HTMLInputElement).value, 10) || 1000)}
            class="field-input mono"
          />
        </label>
      {/if}

      <label class="field">
        <span class="field-label">Config (JSON)</span>
        <textarea class="field-textarea mono" rows="8" onchange={handleConfigChange}
          >{JSON.stringify(node.config, null, 2)}</textarea
        >
      </label>
    </div>

    <div class="panel-footer">
      <button class="btn btn-danger" onclick={() => ondelete?.(node.id)}>Delete Node</button>
    </div>
  </div>
{/if}

<style>
  .config-panel {
    position: fixed;
    top: 0;
    right: 0;
    width: 320px;
    height: 100vh;
    background: var(--color-bg-elevated-1);
    border-left: 1px solid var(--color-border-default);
    display: flex;
    flex-direction: column;
    z-index: 50;
    box-shadow: var(--shadow-elevated-3);
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-3) var(--spacing-4);
    border-bottom: 1px solid var(--color-border-default);
  }

  .panel-header h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
    margin: 0;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    font-size: var(--font-size-md);
    cursor: pointer;
    padding: var(--spacing-1);
  }

  .close-btn:hover {
    color: var(--color-text-primary);
  }

  .panel-body {
    flex: 1;
    overflow-y: auto;
    padding: var(--spacing-4);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .field-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .field-input,
  .field-textarea {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .field-input:focus,
  .field-textarea:focus {
    outline: none;
    border-color: var(--color-interactive-primary);
  }

  .field-textarea {
    resize: vertical;
    min-height: 80px;
  }

  .field-value {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
  }

  .mono {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .panel-footer {
    padding: var(--spacing-3) var(--spacing-4);
    border-top: 1px solid var(--color-border-default);
  }

  .btn {
    width: 100%;
    padding: var(--spacing-2);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .btn-danger {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }
</style>
