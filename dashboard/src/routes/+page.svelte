<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import type { Agent } from '@ghost/sdk';
  import ScoreGauge from '../components/ScoreGauge.svelte';

  let score = $state(0);
  let level = $state(0);
  let agents: Agent[] = $state([]);
  let loading = $state(true);
  let error = $state('');

  async function loadDashboard() {
    loading = true;
    error = '';
    try {
      const client = await getGhostClient();
      const [convData, agentData] = await Promise.all([
        client.convergence.scores(),
        client.agents.list(),
      ]);

      // Fix: unwrap {scores: [...]} wrapper, read correct field names.
      if (convData?.scores?.length > 0) {
        score = convData.scores[0].score ?? 0;
        level = convData.scores[0].level ?? 0;
      }
      agents = agentData ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load dashboard data';
      console.error('Failed to load dashboard data:', e);
    } finally {
      loading = false;
    }
  }

  onMount(async () => {
    await loadDashboard();
  });
</script>

<h1 class="page-title">Dashboard</h1>

{#if loading}
  <div class="grid">
    {#each [1, 2, 3] as _}
      <div class="card skeleton">&nbsp;</div>
    {/each}
  </div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={loadDashboard}>Retry</button>
  </div>
{:else}
  <div class="grid">
    <div class="card">
      <div class="card-label">Composite Score</div>
      <ScoreGauge {score} {level} />
    </div>
    <div class="card">
      <div class="card-label">Intervention Level</div>
      <div class="card-value">{level}</div>
    </div>
    <div class="card">
      <div class="card-label">Active Agents</div>
      <div class="card-value">{agents.length}</div>
    </div>
  </div>
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--layout-card-gap);
  }

  .card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
  }

  .card-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wider);
  }

  .card-value {
    font-size: var(--font-size-2xl);
    font-weight: var(--font-weight-bold);
    margin-top: var(--spacing-2);
    font-variant-numeric: tabular-nums;
  }

  .skeleton {
    min-height: 120px;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }

  .error-state {
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
    .grid { grid-template-columns: 1fr; }
  }
</style>
