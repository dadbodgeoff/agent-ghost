<script lang="ts">
  import CapabilityBadge from './CapabilityBadge.svelte';

  interface DiscoveredAgent {
    name: string;
    description: string;
    endpoint_url: string;
    capabilities: string[];
    trust_score: number;
    version: string;
    reachable: boolean;
  }

  interface Props {
    agent: DiscoveredAgent;
    onSendTask?: (agent: DiscoveredAgent) => void;
  }

  let { agent, onSendTask }: Props = $props();

  let trustColor = $derived(
    agent.trust_score >= 0.8 ? 'var(--color-score-high)' :
    agent.trust_score >= 0.5 ? 'var(--color-score-mid)' :
    'var(--color-score-low)'
  );
</script>

<div class="agent-card" class:unreachable={!agent.reachable}>
  <div class="card-header">
    <div class="agent-info">
      <span class="status-dot" class:online={agent.reachable}></span>
      <span class="agent-name">{agent.name}</span>
    </div>
    <span class="trust-badge" style="color: {trustColor}">
      {(agent.trust_score * 100).toFixed(0)}%
    </span>
  </div>

  <p class="agent-desc">{agent.description}</p>

  <div class="agent-caps">
    {#each agent.capabilities as cap}
      <CapabilityBadge capability={cap} />
    {/each}
  </div>

  <div class="card-footer">
    <span class="agent-url" title={agent.endpoint_url}>
      {agent.endpoint_url.replace(/^https?:\/\//, '')}
    </span>
    <span class="agent-version">v{agent.version}</span>
  </div>

  {#if onSendTask && agent.reachable}
    <button class="send-btn" onclick={() => onSendTask(agent)}>
      Send Task
    </button>
  {/if}
</div>

<style>
  .agent-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    transition: border-color var(--duration-fast) var(--easing-default);
  }

  .agent-card:hover {
    border-color: var(--color-border-emphasis);
  }

  .agent-card.unreachable {
    opacity: 0.6;
  }

  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .agent-info {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-severity-hard);
  }

  .status-dot.online {
    background: var(--color-score-high);
  }

  .agent-name {
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .trust-badge {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-bold);
    font-variant-numeric: tabular-nums;
  }

  .agent-desc {
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    margin: 0;
  }

  .agent-caps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-1);
  }

  .card-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .agent-url {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 70%;
  }

  .agent-version {
    font-variant-numeric: tabular-nums;
  }

  .send-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .send-btn:hover {
    opacity: 0.9;
  }
</style>
