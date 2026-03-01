<script lang="ts">
  /**
   * WorkflowNodeConfig — slide-in config panel for a selected workflow node (T-2.6.2).
   */
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

  const NODE_TYPES = ['llm_call', 'tool_exec', 'gate_check', 'transform', 'condition', 'parallel_branch', 'sub_workflow', 'ab_test'] as const;

  function handleLabelChange(e: Event) {
    if (!node) return;
    const target = e.target as HTMLInputElement;
    onupdate?.({ ...node, label: target.value });
  }

  function handleTypeChange(e: Event) {
    if (!node) return;
    const target = e.target as HTMLSelectElement;
    onupdate?.({ ...node, type: target.value });
  }

  function handleConfigChange(e: Event) {
    if (!node) return;
    const target = e.target as HTMLTextAreaElement;
    try {
      const config = JSON.parse(target.value);
      onupdate?.({ ...node, config });
    } catch {
      // Invalid JSON — don't update
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
          {#each NODE_TYPES as t}
            <option value={t}>{t}</option>
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

      {#if node.type === 'sub_workflow'}
        <label class="field">
          <span class="field-label">Sub-Workflow ID</span>
          <input
            type="text"
            value={node.config?.workflow_id ?? ''}
            onchange={(e) => {
              if (!node) return;
              const target = e.target as HTMLInputElement;
              onupdate?.({ ...node, config: { ...node.config, workflow_id: target.value } });
            }}
            class="field-input mono"
            placeholder="Referenced workflow ID"
          />
        </label>
      {/if}

      {#if node.type === 'ab_test'}
        <div class="field">
          <span class="field-label">Variant A Ratio (%)</span>
          <input
            type="range"
            min="0"
            max="100"
            value={node.config?.variants?.[0]?.ratio ?? 50}
            onchange={(e) => {
              if (!node) return;
              const target = e.target as HTMLInputElement;
              const ratioA = parseInt(target.value);
              const variants = [
                { label: node.config?.variants?.[0]?.label ?? 'A', ratio: ratioA },
                { label: node.config?.variants?.[1]?.label ?? 'B', ratio: 100 - ratioA },
              ];
              onupdate?.({ ...node, config: { ...node.config, variants } });
            }}
          />
          <span class="field-value">
            A: {node.config?.variants?.[0]?.ratio ?? 50}% /
            B: {node.config?.variants?.[1]?.ratio ?? 50}%
          </span>
        </div>
      {/if}

      {#if node.type === 'condition'}
        <label class="field">
          <span class="field-label">Condition Expression</span>
          <input
            type="text"
            value={node.condition ?? ''}
            onchange={(e) => {
              if (!node) return;
              const target = e.target as HTMLInputElement;
              onupdate?.({ ...node, condition: target.value });
            }}
            class="field-input mono"
            placeholder="e.g. score > 0.8"
          />
        </label>
      {/if}

      <label class="field">
        <span class="field-label">Config (JSON)</span>
        <textarea
          class="field-textarea mono"
          rows="6"
          onchange={handleConfigChange}
        >{JSON.stringify(node.config, null, 2)}</textarea>
      </label>
    </div>

    <div class="panel-footer">
      <button class="btn btn-danger" onclick={() => ondelete?.(node!.id)}>
        Delete Node
      </button>
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

  .close-btn:hover { color: var(--color-text-primary); }

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

  .field-input, .field-textarea {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .field-input:focus, .field-textarea:focus {
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
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .btn-danger {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }

  .btn-danger:hover { filter: brightness(1.1); }
</style>
