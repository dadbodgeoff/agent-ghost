<!--
  NodeDetailPanel — Slide-in panel for DAG node details (T-2.2.2).

  Shows full event data when a node in the CausalGraph is clicked.
  Includes GateCheckBar at the bottom for safety gate status.

  Ref: ADE_DESIGN_PLAN §5.2.2, tasks.md T-2.2.2
-->
<script lang="ts">
  import GateCheckBar from './GateCheckBar.svelte';
  import type { JsonObject } from '$lib/types/json';

  interface NodeData {
    id: string;
    label: string;
    type: string;
    event_type?: string;
    sender?: string;
    timestamp?: string;
    sequence_number?: number;
    content_hash?: string;
    latency_ms?: number;
    token_count?: number;
    attributes?: JsonObject;
    gates?: Array<{ name: string; status: 'pass' | 'fail' | 'warning' | 'unknown'; detail?: string }>;
  }

  interface Props {
    node: NodeData | null;
    onclose?: () => void;
  }

  let { node, onclose }: Props = $props();

  let isOpen = $derived(node !== null);

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') onclose?.();
  }

  const typeLabels: Record<string, string> = {
    llm_call: 'LLM Call',
    tool_exec: 'Tool Execution',
    proposal: 'Proposal',
    gate_check: 'Gate Check',
    intervention: 'Intervention',
  };
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if isOpen && node}
  <div class="panel-backdrop" onclick={onclose} role="presentation"></div>
  <div class="node-detail-panel" role="dialog" aria-modal="true" aria-label="Node details">
    <header class="panel-header">
      <div class="header-info">
        <span class="node-type">{typeLabels[node.type] ?? node.type}</span>
        <h3>{node.label}</h3>
      </div>
      <button class="close-btn" onclick={onclose} aria-label="Close panel">
        &times;
      </button>
    </header>

    <div class="panel-body">
      <dl class="detail-list">
        <dt>ID</dt>
        <dd class="mono">{node.id}</dd>

        {#if node.event_type}
          <dt>Event Type</dt>
          <dd>{node.event_type}</dd>
        {/if}

        {#if node.sender}
          <dt>Sender</dt>
          <dd>{node.sender}</dd>
        {/if}

        {#if node.timestamp}
          <dt>Timestamp</dt>
          <dd>{new Date(node.timestamp).toLocaleString()}</dd>
        {/if}

        {#if node.sequence_number !== undefined}
          <dt>Sequence</dt>
          <dd>#{node.sequence_number}</dd>
        {/if}

        {#if node.content_hash}
          <dt>Content Hash</dt>
          <dd class="mono">{node.content_hash}</dd>
        {/if}

        {#if node.latency_ms !== undefined}
          <dt>Latency</dt>
          <dd>{node.latency_ms}ms</dd>
        {/if}

        {#if node.token_count !== undefined}
          <dt>Tokens</dt>
          <dd>{node.token_count.toLocaleString()}</dd>
        {/if}
      </dl>

      {#if node.attributes && Object.keys(node.attributes).length > 0}
        <section class="attributes-section">
          <h4>Attributes</h4>
          <pre class="attributes-json">{JSON.stringify(node.attributes, null, 2)}</pre>
        </section>
      {/if}
    </div>

    <footer class="panel-footer">
      <GateCheckBar gates={node.gates} compact={true} />
    </footer>
  </div>
{/if}

<style>
  .panel-backdrop {
    position: fixed;
    inset: 0;
    background: var(--color-bg-overlay);
    z-index: 90;
  }

  .node-detail-panel {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(400px, 90vw);
    background: var(--color-bg-base);
    border-left: 1px solid var(--color-border-default);
    box-shadow: var(--shadow-elevated-3);
    z-index: 100;
    display: flex;
    flex-direction: column;
    animation: slide-in var(--duration-normal) var(--easing-default);
  }

  @keyframes slide-in {
    from { transform: translateX(100%); }
    to { transform: translateX(0); }
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    padding: var(--spacing-4);
    border-bottom: 1px solid var(--color-border-default);
  }

  .header-info { flex: 1; }

  .node-type {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    color: var(--color-text-muted);
  }

  .header-info h3 {
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-bold);
    margin: var(--spacing-1) 0 0;
  }

  .close-btn {
    background: none;
    border: none;
    font-size: var(--font-size-xl);
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 0;
    line-height: 1;
  }

  .close-btn:hover { color: var(--color-text-primary); }

  .panel-body {
    flex: 1;
    overflow-y: auto;
    padding: var(--spacing-4);
  }

  .detail-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--spacing-1) var(--spacing-3);
    margin: 0;
  }

  .detail-list dt {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    padding-top: 2px;
  }

  .detail-list dd {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    margin: 0;
    word-break: break-all;
  }

  .mono {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .attributes-section {
    margin-top: var(--spacing-4);
  }

  .attributes-section h4 {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-2);
  }

  .attributes-json {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-1);
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    max-height: 200px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .panel-footer {
    padding: var(--spacing-3);
    border-top: 1px solid var(--color-border-default);
  }
</style>
