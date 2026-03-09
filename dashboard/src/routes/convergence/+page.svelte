<script lang="ts">
  /**
   * Enhanced Convergence Dashboard (Phase 3, Task 3.8).
   * 8-axis radar chart, sparkline trends, agent selector, threshold config.
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import ScoreGauge from '../../components/ScoreGauge.svelte';

  interface AgentScore {
    agent_id: string;
    agent_name: string;
    score: number;
    level: number;
    profile: string;
    signal_scores: Record<string, number>;
    computed_at: string | null;
  }

  interface AgentHistoryEntry {
    session_id: string | null;
    score: number;
    level: number;
    profile: string;
    signal_scores: Record<string, number>;
    computed_at: string;
  }

  let scores: AgentScore[] = $state([]);
  let history: AgentHistoryEntry[] = $state([]);
  let loading = $state(true);
  let historyLoading = $state(false);
  let error = $state('');
  let historyError = $state('');
  let monitorOnline = $state(true);
  let lastMonitorUpdate = $state<string | null>(null);
  let selectedAgentId = $state<string | null>(null);
  let showThresholdConfig = $state(false);
  let historyRequestSeq = 0;

  // Threshold config
  let thresholds = $state({ normal: 0.4, elevated: 0.6, high: 0.8 });

  const SIGNAL_NAMES = [
    'session_duration', 'inter_session_gap', 'response_latency',
    'vocabulary_convergence', 'goal_boundary_erosion',
    'initiative_balance', 'disengagement_resistance',
    'behavioral_anomaly',
  ];

  const SIGNAL_LABELS = [
    'Session Duration', 'Inter-Session Gap', 'Response Latency',
    'Vocabulary Convergence', 'Goal Boundary Erosion',
    'Initiative Balance', 'Disengagement Resistance',
    'Behavioral Anomaly',
  ];

  const SIGNAL_SHORT = ['S1', 'S2', 'S3', 'S4', 'S5', 'S6', 'S7', 'S8'];

  const LEVEL_LABELS = ['Normal', 'Soft', 'Active', 'Hard', 'External'];
  const LEVEL_COLORS = [
    'var(--color-severity-normal)',
    'var(--color-severity-soft)',
    'var(--color-severity-active)',
    'var(--color-severity-hard)',
    'var(--color-severity-external)',
  ];

  function signalScoresToArray(obj: Record<string, number>): number[] {
    return SIGNAL_NAMES.map(name => obj[name] ?? 0);
  }

  let selectedAgent = $derived(
    selectedAgentId
      ? scores.find(s => s.agent_id === selectedAgentId) ?? scores[0]
      : scores[0]
  );

  async function loadScores() {
    try {
      const client = await getGhostClient();
      const [scoreData, healthData] = await Promise.all([
        client.convergence.scores(),
        client.health.check().catch(() => null),
      ]);
      scores = scoreData?.scores ?? [];

      if (healthData?.convergence_monitor) {
        monitorOnline = healthData.convergence_monitor.connected === true;
        lastMonitorUpdate = null;
      }

      // Auto-select first agent if none selected
      if (!selectedAgentId && scores.length > 0) {
        selectedAgentId = scores[0].agent_id;
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load convergence data';
    }
    loading = false;
  }

  async function loadHistory(agentId: string) {
    const requestSeq = ++historyRequestSeq;
    historyLoading = true;
    historyError = '';

    try {
      const client = await getGhostClient();
      const result = await client.convergence.history(agentId, { limit: 24 });
      if (requestSeq !== historyRequestSeq) return;
      history = (result.entries ?? []).map((entry) => ({
        ...entry,
        session_id: entry.session_id ?? null,
      }));
    } catch (e: unknown) {
      if (requestSeq !== historyRequestSeq) return;
      history = [];
      historyError = e instanceof Error ? e.message : 'Failed to load convergence history';
    } finally {
      if (requestSeq === historyRequestSeq) {
        historyLoading = false;
      }
    }
  }

  // Radar chart SVG rendering
  function radarPoints(signals: number[], radius: number): string {
    const n = signals.length;
    const angleSlice = (Math.PI * 2) / n;
    return signals.map((val, i) => {
      const angle = angleSlice * i - Math.PI / 2;
      const x = radius * val * Math.cos(angle);
      const y = radius * val * Math.sin(angle);
      return `${x},${y}`;
    }).join(' ');
  }

  function radarAxisEnd(idx: number, total: number, radius: number): { x: number; y: number } {
    const angle = (Math.PI * 2 / total) * idx - Math.PI / 2;
    return { x: radius * Math.cos(angle), y: radius * Math.sin(angle) };
  }

  function radarLabelPos(idx: number, total: number, radius: number): { x: number; y: number } {
    const angle = (Math.PI * 2 / total) * idx - Math.PI / 2;
    return { x: radius * 1.2 * Math.cos(angle), y: radius * 1.2 * Math.sin(angle) };
  }

  // Sparkline path
  function sparklinePath(values: number[], width: number, height: number): string {
    if (values.length < 2) return '';
    const step = width / (values.length - 1);
    return values.map((v, i) => {
      const x = i * step;
      const y = height - v * height;
      return `${i === 0 ? 'M' : 'L'}${x},${y}`;
    }).join(' ');
  }

  function historySignalValues(name: string): number[] {
    return history.map((entry) => Math.max(0, Math.min(1, entry.signal_scores?.[name] ?? 0)));
  }

  onMount(() => {
    loadScores();
    const unsub = wsStore.on('ScoreUpdate', () => { loadScores(); });
    const unsubResync = wsStore.onResync(() => { loadScores(); });
    return () => {
      unsub();
      unsubResync();
    };
  });

  $effect(() => {
    const agentId = selectedAgentId;
    const latestComputedAt = selectedAgent?.computed_at;
    void latestComputedAt;

    if (!agentId) {
      history = [];
      historyError = '';
      return;
    }

    void loadHistory(agentId);
  });
</script>

<div class="page-header">
  <h1 class="page-title">Convergence</h1>
  {#if scores.length > 1}
    <select class="agent-selector" bind:value={selectedAgentId}>
      {#each scores as agent}
        <option value={agent.agent_id}>{agent.agent_name}</option>
      {/each}
    </select>
  {/if}
</div>

{#if !monitorOnline}
  <div class="degraded-banner" role="alert">
    <span class="degraded-icon" aria-hidden="true">!</span>
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
{:else if selectedAgent}
  <!-- Score Gauge + Radar Chart Row -->
  <div class="overview-row">
    <div class="card gauge-card">
      <ScoreGauge score={selectedAgent.score} level={selectedAgent.level} />
      <div class="gauge-label">
        <span class="level-badge" style="color: {LEVEL_COLORS[selectedAgent.level] ?? LEVEL_COLORS[0]}">
          L{selectedAgent.level} — {LEVEL_LABELS[selectedAgent.level] ?? 'Unknown'}
        </span>
      </div>
    </div>

    <div class="card radar-card">
      <h2>Signal Radar</h2>
      {#if true}
      {@const signals = signalScoresToArray(selectedAgent.signal_scores ?? {})}
      {@const R = 120}
      <svg viewBox="-180 -180 360 360" class="radar-svg">
        <!-- Concentric circles -->
        {#each [0.25, 0.5, 0.75, 1.0] as level}
          <circle r={R * level} fill="none" stroke="var(--color-border-default)" stroke-dasharray={level < 1 ? '2,2' : 'none'} stroke-width="0.5" />
        {/each}
        <!-- Axes -->
        {#each signals as _, i}
          {@const end = radarAxisEnd(i, signals.length, R)}
          <line x1="0" y1="0" x2={end.x} y2={end.y} stroke="var(--color-border-subtle)" stroke-width="0.5" />
        {/each}
        <!-- Data polygon -->
        <polygon
          points={radarPoints(signals, R)}
          fill="var(--color-interactive-primary)"
          fill-opacity="0.2"
          stroke="var(--color-interactive-primary)"
          stroke-width="2"
        />
        <!-- Data points -->
        {#each signals as val, i}
          {@const angle = (Math.PI * 2 / signals.length) * i - Math.PI / 2}
          <circle
            cx={R * val * Math.cos(angle)}
            cy={R * val * Math.sin(angle)}
            r="3"
            fill="var(--color-interactive-primary)"
          />
        {/each}
        <!-- Labels -->
        {#each signals as _, i}
          {@const pos = radarLabelPos(i, signals.length, R)}
          <text
            x={pos.x} y={pos.y}
            text-anchor="middle"
            dominant-baseline="middle"
            fill="var(--color-text-muted)"
            font-size="9"
          >{SIGNAL_SHORT[i]}</text>
        {/each}
      </svg>
      {/if}
    </div>
  </div>

  <!-- Signal Trends -->
  <div class="card">
    <h2>Signal Trends (24h)</h2>
    {#if historyError}
      <div class="history-error" role="alert">{historyError}</div>
    {/if}
    <div class="signal-grid">
      {#each SIGNAL_NAMES as name, i}
        {@const val = selectedAgent.signal_scores?.[name] ?? 0}
        {@const signalHistory = historySignalValues(name)}
        <div class="signal-row">
          <span class="signal-id">{SIGNAL_SHORT[i]}</span>
          <span class="signal-name">{SIGNAL_LABELS[i]}</span>
          {#if signalHistory.length > 1}
            <svg class="sparkline" viewBox="0 0 100 20" aria-label={`Persisted history for ${SIGNAL_LABELS[i]}`}>
              <path d={sparklinePath(signalHistory, 100, 20)} fill="none" stroke="var(--color-interactive-primary)" stroke-width="1.5" />
            </svg>
          {:else}
            <span class="signal-history-empty">
              {historyLoading ? 'Loading…' : 'Awaiting history'}
            </span>
          {/if}
          <span class="signal-value" class:signal-high={val > 0.5}>{val.toFixed(2)}</span>
        </div>
      {/each}
    </div>
  </div>

  <!-- S8 Behavioral Anomaly detail -->
  {#if selectedAgent.signal_scores?.behavioral_anomaly !== undefined}
    <div class="card">
      <div class="s8-header">
        <span class="s8-label">S8: Behavioral Anomaly</span>
        <span class="s8-value" class:s8-high={selectedAgent.signal_scores.behavioral_anomaly > 0.5}>
          {(selectedAgent.signal_scores.behavioral_anomaly * 100).toFixed(1)}%
        </span>
      </div>
      <div class="s8-bar-track">
        <div
          class="s8-bar-fill"
          class:s8-high={selectedAgent.signal_scores.behavioral_anomaly > 0.5}
          style="width: {Math.min(selectedAgent.signal_scores.behavioral_anomaly * 100, 100)}%"
        ></div>
      </div>
      <span class="s8-hint">
        {selectedAgent.signal_scores.behavioral_anomaly === 0
          ? 'Calibrating baseline (returns 0.0 until established)'
          : selectedAgent.signal_scores.behavioral_anomaly > 0.5
            ? 'Significant deviation from baseline tool usage patterns'
            : 'Within normal behavioral range'}
      </span>
    </div>
  {/if}

  <!-- Threshold Configuration -->
  <div class="card">
    <div class="section-header">
      <h2>Threshold Configuration</h2>
      <button class="btn-text" onclick={() => (showThresholdConfig = !showThresholdConfig)}>
        {showThresholdConfig ? 'Hide' : 'Adjust Thresholds'}
      </button>
    </div>
    <div class="threshold-summary">
      Normal: &lt; {thresholds.normal} &nbsp; Elevated: &lt; {thresholds.elevated} &nbsp; High: &lt; {thresholds.high}
    </div>
    {#if showThresholdConfig}
      <div class="threshold-sliders">
        <label>
          <span>Normal threshold</span>
          <input type="range" min="0" max="1" step="0.05" bind:value={thresholds.normal} />
          <span class="threshold-val">{thresholds.normal}</span>
        </label>
        <label>
          <span>Elevated threshold</span>
          <input type="range" min="0" max="1" step="0.05" bind:value={thresholds.elevated} />
          <span class="threshold-val">{thresholds.elevated}</span>
        </label>
        <label>
          <span>High threshold</span>
          <input type="range" min="0" max="1" step="0.05" bind:value={thresholds.high} />
          <span class="threshold-val">{thresholds.high}</span>
        </label>
      </div>
    {/if}
  </div>

  {#if selectedAgent.computed_at}
    <div class="timestamp">
      Last computed: {new Date(selectedAgent.computed_at).toLocaleString()}
    </div>
  {/if}
{/if}

<style>
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-6);
  }

  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
  }

  .agent-selector {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .overview-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
    margin-bottom: var(--spacing-4);
  }

  .card h2 {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-3);
  }

  .history-error {
    margin-bottom: var(--spacing-3);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
  }

  .gauge-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  .signal-history-empty {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    min-width: 100px;
    text-align: center;
  }

  .gauge-label {
    margin-top: var(--spacing-3);
    text-align: center;
  }

  .level-badge {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
  }

  .radar-card {
    display: flex;
    flex-direction: column;
    align-items: center;
  }

  .radar-svg {
    width: 100%;
    max-width: 300px;
    height: auto;
  }

  .signal-grid {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .signal-row {
    display: grid;
    grid-template-columns: 30px 1fr 120px 50px;
    gap: var(--spacing-2);
    align-items: center;
    padding: var(--spacing-1) 0;
  }

  .signal-id {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-bold);
    color: var(--color-interactive-primary);
    font-family: var(--font-family-mono);
  }

  .signal-name {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .sparkline {
    height: 20px;
    width: 100%;
  }

  .signal-value {
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    font-weight: var(--font-weight-semibold);
    text-align: right;
    color: var(--color-text-primary);
  }

  .signal-value.signal-high {
    color: var(--color-severity-hard);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-3);
  }

  .section-header h2 { margin-bottom: 0; }

  .btn-text {
    background: none;
    border: none;
    color: var(--color-interactive-primary);
    font-size: var(--font-size-xs);
    cursor: pointer;
    text-decoration: underline;
  }

  .threshold-summary {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-2);
  }

  .threshold-sliders {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    margin-top: var(--spacing-3);
  }

  .threshold-sliders label {
    display: flex;
    align-items: center;
    gap: var(--spacing-3);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .threshold-sliders label span:first-child {
    width: 140px;
  }

  .threshold-sliders input[type="range"] {
    flex: 1;
    accent-color: var(--color-interactive-primary);
  }

  .threshold-val {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    width: 40px;
    text-align: right;
  }

  .timestamp {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-top: var(--spacing-2);
  }

  /* S8 detail */
  .s8-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-2);
  }

  .s8-label {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
  }

  .s8-value {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
    font-family: var(--font-family-mono);
  }
  .s8-value.s8-high { color: var(--color-severity-hard); }

  .s8-bar-track {
    width: 100%;
    height: 6px;
    background: var(--color-bg-elevated-1);
    border-radius: 3px;
    overflow: hidden;
  }

  .s8-bar-fill {
    height: 100%;
    background: var(--color-interactive-primary);
    border-radius: 3px;
    transition: width 0.3s ease;
  }
  .s8-bar-fill.s8-high { background: var(--color-severity-hard); }

  .s8-hint {
    display: block;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-top: var(--spacing-1);
  }

  /* States */
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

  .degraded-icon { font-size: var(--font-size-md); }
  .degraded-time {
    margin-left: auto;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  @media (max-width: 640px) {
    .overview-row { grid-template-columns: 1fr; }
    .signal-row { grid-template-columns: 30px 1fr 60px 40px; }
  }
</style>
