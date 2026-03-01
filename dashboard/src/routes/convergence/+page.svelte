<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import ScoreGauge from '../../components/ScoreGauge.svelte';
  import SignalChart from '../../components/SignalChart.svelte';

  interface AgentScore {
    agent_id: string;
    agent_name: string;
    score: number;
    level: number;
    profile: string;
    signal_scores: Record<string, number>;
    computed_at: string | null;
  }

  let scores: AgentScore[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let monitorOnline = $state(true);
  let lastMonitorUpdate = $state<string | null>(null);

  // Signal names in display order (matches convergence-monitor pipeline).
  const SIGNAL_NAMES = [
    'session_duration', 'inter_session_gap', 'response_latency',
    'vocabulary_convergence', 'goal_boundary_erosion',
    'initiative_balance', 'disengagement_resistance',
  ];

  const SIGNAL_LABELS = [
    'Session Duration', 'Inter-Session Gap', 'Response Latency',
    'Vocabulary Convergence', 'Goal Boundary Erosion',
    'Initiative Balance', 'Disengagement Resistance',
  ];

  const LEVEL_LABELS = ['Normal', 'Soft', 'Active', 'Hard', 'External'];
  const LEVEL_COLORS = [
    'var(--color-severity-normal)',
    'var(--color-severity-soft)',
    'var(--color-severity-active)',
    'var(--color-severity-hard)',
    'var(--color-severity-external)',
  ];

  /** Transform signal_scores JSON object → ordered number[] for SignalChart. */
  function signalScoresToArray(obj: Record<string, number>): number[] {
    return SIGNAL_NAMES.map(name => obj[name] ?? 0);
  }

  onMount(async () => {
    try {
      const [scoreData, healthData] = await Promise.all([
        api.get('/api/convergence/scores'),
        api.get('/api/health').catch(() => null),
      ]);
      scores = scoreData?.scores ?? [];

      // Check monitor connectivity from health endpoint.
      if (healthData?.convergence_monitor) {
        monitorOnline = healthData.convergence_monitor.connected === true;
        lastMonitorUpdate = healthData.convergence_monitor.last_update ?? null;
      }
    } catch (e: any) {
      error = e.message || 'Failed to load convergence data';
    }
    loading = false;
  });
</script>

<h1 class="page-title">Convergence</h1>

{#if !monitorOnline}
  <div class="degraded-banner" role="alert">
    <span class="degraded-icon" aria-hidden="true">⚠</span>
    <span>Monitor offline — data may be stale.</span>
    {#if lastMonitorUpdate}
      <span class="degraded-time">Last update: {new Date(lastMonitorUpdate).toLocaleString()}</span>
    {/if}
  </div>
{/if}

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={() => location.reload()}>Retry</button>
  </div>
{:else if scores.length === 0}
  <div class="empty-state">
    <p>No convergence data yet. Scores appear after agents run.</p>
  </div>
{:else}
  {#each scores as agent}
    <div class="agent-score-card">
      <div class="agent-header">
        <span class="agent-name">{agent.agent_name}</span>
        <span
          class="level-badge"
          style="color: {LEVEL_COLORS[agent.level] ?? LEVEL_COLORS[0]}"
        >
          L{agent.level} — {LEVEL_LABELS[agent.level] ?? 'Unknown'}
        </span>
      </div>

      <div class="score-section">
        <ScoreGauge score={agent.score} level={agent.level} />
      </div>

      <div class="signals-section">
        <SignalChart signals={signalScoresToArray(agent.signal_scores ?? {})} />
      </div>

      {#if agent.computed_at}
        <div class="timestamp">
          Last computed: {new Date(agent.computed_at).toLocaleString()}
        </div>
      {/if}
    </div>
  {/each}
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  .agent-score-card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
    margin-bottom: var(--spacing-4);
  }

  .agent-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-4);
  }

  .agent-name {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
  }

  .level-badge {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
  }

  .score-section {
    margin-bottom: var(--spacing-4);
  }

  .signals-section {
    margin-bottom: var(--spacing-3);
  }

  .timestamp {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .skeleton-block {
    height: 300px;
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

  .degraded-banner {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-severity-soft-bg);
    border: 1px solid var(--color-severity-soft);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
    font-size: var(--font-size-sm);
    color: var(--color-severity-soft);
  }

  .degraded-icon {
    font-size: var(--font-size-md);
  }

  .degraded-time {
    margin-left: auto;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
</style>
