<script lang="ts">
  import { onMount } from 'svelte';
  import { costsStore } from '$lib/stores/costs.svelte';
  import type { AgentCostInfo } from '@ghost/sdk';

  onMount(() => {
    void costsStore.init();
  });

  let costs = $derived(costsStore.costs);
  let loading = $derived(costsStore.loading);
  let error = $derived(costsStore.error);

  function remaining(c: AgentCostInfo): number {
    const spendingCap = c.spending_cap ?? 0;
    return Math.max(0, c.cap_remaining ?? spendingCap - c.daily_total);
  }

  function utilization(c: AgentCostInfo): number {
    if (c.cap_utilization_pct !== undefined) return Math.min(100, c.cap_utilization_pct);
    const spendingCap = c.spending_cap ?? 0;
    if (spendingCap <= 0) return 0;
    return Math.min(100, (c.daily_total / spendingCap) * 100);
  }

  function utilizationColor(pct: number): string {
    if (pct >= 95) return 'var(--color-severity-hard)';
    if (pct >= 80) return 'var(--color-severity-soft)';
    return 'var(--color-brand-primary)';
  }

  function formatCost(val: number): string {
    return `$${val.toFixed(4)}`;
  }
</script>

<h1 class="page-title">Costs</h1>

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={() => void costsStore.refresh(true)}>Retry</button>
  </div>
{:else if costs.length === 0}
  <div class="empty-state">
    <p>No cost data yet. Costs appear after agents make LLM calls.</p>
  </div>
{:else}
  <div class="cost-grid">
    {#each costs as agent (agent.agent_id)}
      {@const pct = utilization(agent)}
      {@const color = utilizationColor(pct)}
      <div class="cost-card">
        <div class="cost-header">
          <span class="agent-name">{agent.agent_name || agent.agent_id.slice(0, 8)}</span>
          <span class="utilization" style="color: {color}">{pct.toFixed(1)}%</span>
        </div>

        <div class="bar-track">
          <div
            class="bar-fill"
            style="width: {pct}%; background: {color}"
            role="progressbar"
            aria-valuenow={pct}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-label="Spending utilization for {agent.agent_name || agent.agent_id}"
          ></div>
        </div>

        <div class="cost-details">
          <div class="detail-row">
            <span class="detail-label">Daily Total</span>
            <span class="detail-value">{formatCost(agent.daily_total)}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">Compaction</span>
            <span class="detail-value">{formatCost(agent.compaction_cost)}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">Cap</span>
            <span class="detail-value">{formatCost(agent.spending_cap)}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">Remaining</span>
            <span class="detail-value remaining">{formatCost(remaining(agent))}</span>
          </div>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  .cost-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--layout-card-gap);
  }

  .cost-card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
  }

  .cost-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-3);
  }

  .agent-name {
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
  }

  .utilization {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-bold);
    font-variant-numeric: tabular-nums;
  }

  .bar-track {
    height: 6px;
    background: var(--color-border-default);
    border-radius: var(--radius-full);
    overflow: hidden;
    margin-bottom: var(--spacing-4);
  }

  .bar-fill {
    height: 100%;
    border-radius: var(--radius-full);
    transition: width var(--duration-normal) var(--easing-default);
  }

  .cost-details {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    font-size: var(--font-size-sm);
  }

  .detail-label {
    color: var(--color-text-muted);
  }

  .detail-value {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .remaining {
    color: var(--color-severity-normal);
  }

  .skeleton-block {
    height: 200px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }

  .empty-state, .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .error-state button {
    margin-top: var(--spacing-4);
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
  }

  @media (max-width: 640px) {
    .cost-grid { grid-template-columns: 1fr; }
  }
</style>
