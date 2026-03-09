<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import type { Agent, ConvergenceScore } from '@ghost/sdk';
  import ScoreGauge from '../../components/ScoreGauge.svelte';

  let agents: Agent[] = $state([]);
  let scoreMap: Map<string, ConvergenceScore> = $state(new Map());
  let loading = $state(true);
  let error = $state('');

  const STATUS_LABELS: Record<string, string> = {
    Starting: 'Starting',
    Running: 'Running',
    Stopping: 'Stopping',
    Stopped: 'Stopped',
  };

  async function loadAgents() {
    try {
      const client = await getGhostClient();
      const [agentData, convData] = await Promise.all([
        client.agents.list(),
        client.convergence.scores().catch(() => ({ scores: [] })),
      ]);
      agents = agentData ?? [];
      const scores: ConvergenceScore[] = convData?.scores ?? [];
      scoreMap = new Map(scores.map((s: ConvergenceScore) => [s.agent_id, s]));
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load agents';
    }
    loading = false;
  }

  onMount(() => {
    loadAgents();

    // T-5.9.1: Wire AgentStateChange to refresh agent list.
    const unsub = wsStore.on('AgentStateChange', () => { loadAgents(); });
    const unsubResync = wsStore.onResync(() => { loadAgents(); });
    return () => {
      unsub();
      unsubResync();
    };
  });

  function getScore(agentId: string): ConvergenceScore | undefined {
    return scoreMap.get(agentId);
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'Running': return 'var(--color-severity-normal)';
      case 'Starting': return 'var(--color-severity-soft)';
      case 'Stopping': return 'var(--color-severity-active)';
      case 'Stopped': return 'var(--color-text-disabled)';
      default: return 'var(--color-text-muted)';
    }
  }
</script>

<h1 class="page-title">Agents</h1>

{#if loading}
  <div class="grid">
    {#each [1, 2, 3] as _}
      <div class="card skeleton">&nbsp;</div>
    {/each}
  </div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={() => location.reload()}>Retry</button>
  </div>
{:else if agents.length === 0}
  <div class="empty-state">
    <p>No agents registered yet.</p>
  </div>
{:else}
  <div class="grid">
    {#each agents as agent (agent.id)}
      {@const agentScore = getScore(agent.id)}
      <a href={`/agents/${agent.id}`} class="agent-card" class:inactive={agent.status === 'Stopped'}>
        <div class="agent-header">
          <span class="agent-name">{agent.name}</span>
          <span class="status-badge" style="color: {statusColor(agent.status)}">
            {STATUS_LABELS[agent.status] ?? agent.status}
          </span>
        </div>

        {#if agentScore}
          <div class="gauge-section">
            <ScoreGauge score={agentScore.score} level={agentScore.level} />
          </div>
        {:else}
          <div class="no-score">No convergence data</div>
        {/if}

        <div class="agent-meta">
          {#if agent.spending_cap != null}
            <div class="meta-row">
              <span class="meta-label">Spending Cap</span>
              <span class="meta-value">${agent.spending_cap.toFixed(2)}</span>
            </div>
          {/if}
        </div>
      </a>
    {/each}
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
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: var(--layout-card-gap);
  }

  .agent-card {
    display: block;
    text-decoration: none;
    color: inherit;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
    transition: border-color var(--duration-fast) var(--easing-default);
  }

  .agent-card:hover {
    border-color: var(--color-border-strong);
  }

  .agent-card.inactive {
    opacity: 0.5;
  }

  .agent-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-3);
  }

  .agent-name {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
  }

  .status-badge {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .gauge-section {
    margin-bottom: var(--spacing-3);
  }

  .no-score {
    text-align: center;
    padding: var(--spacing-6) 0;
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .agent-meta {
    border-top: 1px solid var(--color-border-subtle);
    padding-top: var(--spacing-3);
  }

  .meta-row {
    display: flex;
    justify-content: space-between;
    font-size: var(--font-size-sm);
    margin-bottom: var(--spacing-1);
  }

  .meta-label {
    color: var(--color-text-muted);
  }

  .meta-value {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .skeleton {
    min-height: 200px;
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
    .grid { grid-template-columns: 1fr; }
  }
</style>
