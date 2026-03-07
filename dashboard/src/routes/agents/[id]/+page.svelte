<script lang="ts">
  /**
   * Agent detail view — convergence history, cost breakdown, session list,
   * audit entries, and lifecycle controls (pause/resume/quarantine).
   *
   * Ref: tasks.md T-1.10.3, ADE_DESIGN_PLAN §5.3
  */
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api';
  import { getGhostClient } from '$lib/ghost-client';
  import type {
    AgentCostInfo,
    AuditEntry as AuditLogEntry,
    ConvergenceScore,
    ListRuntimeSessionsCursorResult,
    ListRuntimeSessionsPageResult,
    RuntimeSession,
  } from '@ghost/sdk';
  import ScoreGauge from '../../../components/ScoreGauge.svelte';
  import CostBar from '../../../components/CostBar.svelte';
  import ConfirmDialog from '../../../components/ConfirmDialog.svelte';

  interface AgentDetail {
    id: string;
    name: string;
    status: string;
    spending_cap: number;
    capabilities?: string[];
  }

  let agentId = $derived($page.params.id);
  let agent: AgentDetail | null = $state(null);
  let score: ConvergenceScore | null = $state(null);
  let cost: AgentCostInfo | null = $state(null);
  let sessions: RuntimeSession[] = $state([]);
  let auditEntries: AuditLogEntry[] = $state([]);
  let crdtState: any = $state(null);
  let integrityReport: any = $state(null);
  let loading = $state(true);
  let error = $state('');

  // Lifecycle control state
  let confirmAction: 'pause' | 'resume' | 'quarantine' | null = $state(null);
  let actionLoading = $state(false);
  let actionError = $state('');

  onMount(async () => {
    await loadData();
  });

  async function loadData() {
    try {
      loading = true;
      error = '';
      const client = await getGhostClient();

      const [agentsData, convData, costsData, sessionsData, auditData, crdtData, integrityData] = await Promise.all([
        client.agents.list(),
        client.convergence.scores().catch(() => ({ scores: [] })),
        client.costs.list().catch(() => []),
        client.runtimeSessions.list({ page_size: 10 }).catch(() => ({ sessions: [] })),
        client.audit.query({ agent_id: agentId, page_size: 20 }).catch(() => ({ entries: [] })),
        api.get(`/api/state/crdt/${agentId}?limit=50`).catch(() => null),
        api.get(`/api/integrity/chain/${agentId}`).catch(() => null),
      ]);

      const agents: AgentDetail[] = agentsData ?? [];
      agent = agents.find(a => a.id === agentId) ?? null;

      if (!agent) {
        error = 'Agent not found';
        loading = false;
        return;
      }

      const scores: ConvergenceScore[] = convData?.scores ?? [];
      score = scores.find(s => s.agent_id === agentId) ?? null;

      const allCosts: AgentCostInfo[] = costsData ?? [];
      cost = allCosts.find(c => c.agent_id === agentId) ?? null;

      const allSessions = getSessions(sessionsData);
      sessions = allSessions.filter(s =>
        s.session_id && typeof s.session_id === 'string'
      ).slice(0, 10);

      auditEntries = auditData?.entries ?? [];
      crdtState = crdtData;
      integrityReport = integrityData;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load agent details';
    }
    loading = false;
  }

  async function executeAction(action: 'pause' | 'resume' | 'quarantine') {
    actionLoading = true;
    actionError = '';
    try {
      const client = await getGhostClient();
      if (action === 'pause') {
        await client.safety.pause(agentId, 'Paused from dashboard');
      } else if (action === 'resume') {
        await client.safety.resume(agentId);
      } else {
        await client.safety.quarantine(agentId, 'Quarantined from dashboard');
      }
      confirmAction = null;
      await loadData();
    } catch (e: unknown) {
      actionError = e instanceof Error ? e.message : `Failed to ${action} agent`;
    }
    actionLoading = false;
  }

  function statusColor(status: string): string {
    switch (status?.toLowerCase()) {
      case 'active': case 'running': case 'starting': return 'var(--color-severity-normal)';
      case 'paused': return 'var(--color-severity-soft)';
      case 'quarantined': return 'var(--color-severity-hard)';
      case 'deleted': case 'stopped': return 'var(--color-text-disabled)';
      default: return 'var(--color-text-muted)';
    }
  }

  function severityColor(severity: string): string {
    switch (severity) {
      case 'critical': return 'var(--color-severity-hard)';
      case 'error': return 'var(--color-severity-active)';
      case 'warning': return 'var(--color-severity-soft)';
      default: return 'var(--color-text-muted)';
    }
  }

  const signalLabels: Record<string, string> = {
    goal_alignment: 'Goal Alignment',
    behavioral_consistency: 'Behavioral Consistency',
    resource_efficiency: 'Resource Efficiency',
    safety_compliance: 'Safety Compliance',
    output_quality: 'Output Quality',
    collaboration: 'Collaboration',
    learning_rate: 'Learning Rate',
  };

  function getSessions(
    data: ListRuntimeSessionsPageResult | ListRuntimeSessionsCursorResult | { sessions?: RuntimeSession[] },
  ): RuntimeSession[] {
    if ('sessions' in data) return data.sessions ?? [];
    return data.data;
  }
</script>

{#if loading}
  <div class="loading-state">Loading agent details…</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <a href="/agents" class="back-link">← Back to agents</a>
  </div>
{:else if agent}
  <div class="detail-header">
    <a href="/agents" class="back-link">← Agents</a>
    <div class="header-row">
      <h1>{agent.name}</h1>
      <span class="status-badge" style="color: {statusColor(agent.status)}">{agent.status}</span>
    </div>
  </div>

  <!-- Lifecycle Controls -->
  <div class="controls">
    {#if agent.status !== 'Paused' && agent.status !== 'Quarantined'}
      <button class="btn btn-warning" onclick={() => confirmAction = 'pause'}>Pause</button>
    {/if}
    {#if agent.status === 'Paused'}
      <button class="btn btn-success" onclick={() => confirmAction = 'resume'}>Resume</button>
    {/if}
    {#if agent.status !== 'Quarantined'}
      <button class="btn btn-danger" onclick={() => confirmAction = 'quarantine'}>Quarantine</button>
    {/if}
    {#if actionError}
      <span class="action-error">{actionError}</span>
    {/if}
  </div>

  <div class="detail-grid">
    <!-- Convergence Score -->
    <section class="card">
      <h2>Convergence</h2>
      {#if score}
        <div class="gauge-center">
          <ScoreGauge score={score.score} level={score.level} />
        </div>
        {#if score.signal_scores && Object.keys(score.signal_scores).length > 0}
          <div class="signal-list">
            {#each Object.entries(score.signal_scores) as [key, value]}
              <div class="signal-row">
                <span class="signal-name">{signalLabels[key] ?? key}</span>
                <span class="signal-value">{typeof value === 'number' ? value.toFixed(2) : value}</span>
              </div>
            {/each}
          </div>
        {/if}
        {#if score.computed_at}
          <p class="timestamp">Updated {new Date(score.computed_at).toLocaleString()}</p>
        {/if}
      {:else}
        <p class="no-data">No convergence data available</p>
      {/if}
    </section>

    <!-- Cost Breakdown -->
    <section class="card">
      <h2>Costs</h2>
      {#if cost}
        <div class="cost-rows">
          <div class="cost-row">
            <span class="cost-label">Daily Total</span>
            <span class="cost-value">${cost.daily_total.toFixed(4)}</span>
          </div>
          <div class="cost-row">
            <span class="cost-label">Compaction</span>
            <span class="cost-value">${cost.compaction_cost.toFixed(4)}</span>
          </div>
          <div class="cost-row">
            <span class="cost-label">Cap</span>
            <span class="cost-value">${cost.spending_cap.toFixed(2)}</span>
          </div>
          <div class="cost-row">
            <span class="cost-label">Remaining</span>
            <span class="cost-value">${cost.cap_remaining.toFixed(2)}</span>
          </div>
        </div>
        <div class="utilization-bar">
          <CostBar utilization={cost.cap_utilization_pct} />
        </div>
      {:else}
        <p class="no-data">No cost data available</p>
      {/if}
    </section>

    <!-- Recent Sessions -->
    <section class="card">
      <h2>Recent Sessions</h2>
      {#if sessions.length > 0}
        <div class="session-list">
          {#each sessions as session}
            <div class="session-row">
              <span class="session-id" title={session.session_id}>
                {session.session_id.slice(0, 8)}…
              </span>
              <span class="session-events">{session.event_count} events</span>
              <span class="session-time">{new Date(session.started_at).toLocaleDateString()}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="no-data">No sessions found</p>
      {/if}
    </section>

    <!-- Audit Log -->
    <section class="card">
      <h2>Audit Log</h2>
      {#if auditEntries.length > 0}
        <div class="audit-list">
          {#each auditEntries as entry}
            <div class="audit-row">
              <span class="audit-severity" style="color: {severityColor(entry.severity)}">
                {entry.severity}
              </span>
              <span class="audit-type">{entry.event_type}</span>
              <span class="audit-time">{new Date(entry.timestamp).toLocaleString()}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="no-data">No audit entries for this agent</p>
      {/if}
    </section>

    <!-- CRDT State (T-2.3.2) -->
    <section class="card">
      <h2>CRDT State</h2>
      {#if crdtState && crdtState.deltas && crdtState.deltas.length > 0}
        <div class="crdt-meta">
          <span>{crdtState.total} total deltas</span>
          <span class="chain-indicator" class:valid={crdtState.chain_valid} class:broken={!crdtState.chain_valid}>
            Chain: {crdtState.chain_valid ? 'Valid' : 'Broken'}
          </span>
        </div>
        <div class="crdt-list">
          {#each crdtState.deltas.slice(0, 20) as delta}
            <div class="crdt-row">
              <span class="crdt-type">{delta.event_type}</span>
              <span class="crdt-memory">{delta.memory_id?.slice(0, 8)}…</span>
              <span class="crdt-time">{new Date(delta.recorded_at).toLocaleString()}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="no-data">No CRDT deltas for this agent</p>
      {/if}
    </section>

    <!-- Hash Chain Inspector (T-2.3.3) -->
    <section class="card wide">
      <h2>Hash Chain Integrity</h2>
      {#if integrityReport && integrityReport.chains}
        {@const chains = integrityReport.chains}
        {#if chains.itp_events}
          <div class="integrity-section">
            <h3>ITP Events</h3>
            <div class="integrity-meta">
              <span>{chains.itp_events.total_events} events across {chains.itp_events.sessions_checked} sessions</span>
              <span class="chain-indicator" class:valid={chains.itp_events.is_valid} class:broken={!chains.itp_events.is_valid}>
                {chains.itp_events.is_valid ? 'Valid' : `${chains.itp_events.breaks.length} breaks`}
              </span>
            </div>
          </div>
        {/if}
        {#if chains.memory_events}
          <div class="integrity-section">
            <h3>Memory Events</h3>
            <div class="integrity-meta">
              <span>{chains.memory_events.total_events} events across {chains.memory_events.memory_chains_checked} chains</span>
              <span class="chain-indicator" class:valid={chains.memory_events.is_valid} class:broken={!chains.memory_events.is_valid}>
                {chains.memory_events.is_valid ? 'Valid' : `${chains.memory_events.breaks.length} breaks`}
              </span>
            </div>
          </div>
        {/if}
      {:else}
        <p class="no-data">No integrity data available</p>
      {/if}
    </section>
  </div>

  <!-- Confirm Dialog -->
  {#if confirmAction}
    <ConfirmDialog
      title="{confirmAction.charAt(0).toUpperCase() + confirmAction.slice(1)} Agent"
      message="Are you sure you want to {confirmAction} agent '{agent.name}'?"
      danger={confirmAction === 'quarantine'}
      loading={actionLoading}
      onconfirm={() => executeAction(confirmAction!)}
      oncancel={() => { confirmAction = null; actionError = ''; }}
    />
  {/if}
{/if}

<style>
  .loading-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-severity-hard);
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-brand-primary);
    text-decoration: none;
  }

  .back-link:hover {
    text-decoration: underline;
  }

  .detail-header {
    margin-bottom: var(--spacing-4);
  }

  .header-row {
    display: flex;
    align-items: center;
    gap: var(--spacing-3);
    margin-top: var(--spacing-2);
  }

  .header-row h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin: 0;
  }

  .status-badge {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .controls {
    display: flex;
    gap: var(--spacing-2);
    align-items: center;
    margin-bottom: var(--spacing-6);
  }

  .btn {
    padding: var(--spacing-1) var(--spacing-3);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
    transition: opacity var(--duration-fast) var(--easing-default);
  }

  .btn:hover { opacity: 0.85; }

  .btn:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus-ring);
  }

  .btn-warning {
    background: var(--color-severity-soft);
    color: var(--color-text-inverse);
  }

  .btn-success {
    background: var(--color-severity-normal);
    color: var(--color-text-inverse);
  }

  .btn-danger {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }

  .action-error {
    font-size: var(--font-size-sm);
    color: var(--color-severity-hard);
  }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--layout-card-gap);
  }

  .card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
  }

  .card h2 {
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-secondary);
    margin-bottom: var(--spacing-3);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .gauge-center {
    display: flex;
    justify-content: center;
    margin-bottom: var(--spacing-3);
  }

  .signal-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .signal-row {
    display: flex;
    justify-content: space-between;
    font-size: var(--font-size-sm);
  }

  .signal-name { color: var(--color-text-secondary); }

  .signal-value {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    color: var(--color-text-primary);
  }

  .timestamp {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-top: var(--spacing-2);
  }

  .no-data {
    text-align: center;
    padding: var(--spacing-6) 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .cost-rows {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    margin-bottom: var(--spacing-3);
  }

  .cost-row {
    display: flex;
    justify-content: space-between;
    font-size: var(--font-size-sm);
  }

  .cost-label { color: var(--color-text-muted); }

  .cost-value {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .utilization-bar {
    margin-top: var(--spacing-2);
  }

  .session-list, .audit-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .session-row, .audit-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: var(--font-size-sm);
    padding: var(--spacing-1) 0;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .session-row:last-child, .audit-row:last-child {
    border-bottom: none;
  }

  .session-id {
    font-family: var(--font-family-mono);
    color: var(--color-brand-primary);
  }

  .session-events { color: var(--color-text-secondary); }
  .session-time { color: var(--color-text-muted); }

  .audit-severity {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    min-width: 60px;
  }

  .audit-type { color: var(--color-text-secondary); flex: 1; }
  .audit-time { color: var(--color-text-muted); font-size: var(--font-size-xs); }

  .card.wide { grid-column: 1 / -1; }

  .crdt-meta, .integrity-meta {
    display: flex;
    justify-content: space-between;
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    margin-bottom: var(--spacing-2);
  }

  .chain-indicator.valid { color: var(--color-severity-normal); font-weight: var(--font-weight-semibold); }
  .chain-indicator.broken { color: var(--color-severity-hard); font-weight: var(--font-weight-semibold); }

  .crdt-list { display: flex; flex-direction: column; gap: var(--spacing-1); }

  .crdt-row {
    display: flex;
    justify-content: space-between;
    font-size: var(--font-size-sm);
    padding: var(--spacing-1) 0;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .crdt-type { color: var(--color-text-primary); }
  .crdt-memory { font-family: var(--font-family-mono); color: var(--color-text-muted); font-size: var(--font-size-xs); }
  .crdt-time { color: var(--color-text-disabled); font-size: var(--font-size-xs); }

  .integrity-section { margin-bottom: var(--spacing-3); }
  .integrity-section h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    margin-bottom: var(--spacing-1);
  }

  @media (max-width: 640px) {
    .detail-grid { grid-template-columns: 1fr; }
    .controls { flex-wrap: wrap; }
  }
</style>
