<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import type { JsonObject } from '$lib/types/json';
  import type { AgentDetail, AgentOverview } from '@ghost/sdk';
  import ScoreGauge from '../../../components/ScoreGauge.svelte';
  import CostBar from '../../../components/CostBar.svelte';
  import ConfirmDialog from '../../../components/ConfirmDialog.svelte';

  type ConfirmAction = 'pause' | 'quarantine' | 'resume_pause' | 'resume_quarantine' | null;

  interface IntegrityChainSummary {
    total_events?: number;
    sessions_checked?: number;
    memory_chains_checked?: number;
    is_valid?: boolean;
    breaks?: unknown[];
  }

  interface IntegrityChains extends JsonObject {
    itp_events?: IntegrityChainSummary;
    memory_events?: IntegrityChainSummary;
  }

  let agentId = $derived($page.params.id);
  let agent: AgentDetail | null = $state(null);
  let overview: AgentOverview | null = $state(null);
  let loading = $state(true);
  let error = $state('');

  let confirmAction: ConfirmAction = $state(null);
  let actionLoading = $state(false);
  let actionError = $state('');
  let sandboxSaving = $state(false);
  let sandboxError = $state('');
  let sandboxMessage = $state('');
  let forensicReviewed = $state(false);
  let secondConfirmation = $state(false);

  let editableSandbox = $state({
    enabled: true,
    mode: 'workspace_write' as 'off' | 'read_only' | 'workspace_write' | 'strict',
    on_violation: 'pause' as 'warn' | 'pause' | 'quarantine' | 'kill_all',
    network_access: false,
    allowed_shell_prefixes: [] as string[],
  });

  const STATE_LABELS: Record<string, string> = {
    starting: 'Starting',
    ready: 'Ready',
    paused: 'Paused',
    quarantined: 'Quarantined',
    kill_all_blocked: 'Kill-All Blocked',
    stopping: 'Stopping',
    stopped: 'Stopped',
  };

  const signalLabels: Record<string, string> = {
    goal_alignment: 'Goal Alignment',
    behavioral_consistency: 'Behavioral Consistency',
    resource_efficiency: 'Resource Efficiency',
    safety_compliance: 'Safety Compliance',
    output_quality: 'Output Quality',
    collaboration: 'Collaboration',
    learning_rate: 'Learning Rate',
  };

  onMount(() => {
    void loadData();
    const unsubStatus = wsStore.on('AgentOperationalStatusChanged', (msg) => {
      if (msg.agent_id === agentId) {
        void loadData();
      }
    });
    const unsubLegacy = wsStore.on('AgentStateChange', (msg) => {
      if (msg.agent_id === agentId) {
        void loadData();
      }
    });
    const unsubCost = wsStore.on('CostUpdate', (msg) => {
      if (msg.agent_id === agentId) {
        void loadData();
      }
    });
    const unsubCostReset = wsStore.on('CostDailyReset', () => {
      void loadData();
    });
    const unsubResync = wsStore.onResync(() => { void loadData(); });
    return () => {
      unsubStatus();
      unsubLegacy();
      unsubCost();
      unsubCostReset();
      unsubResync();
    };
  });

  async function loadData() {
    if (!agentId) {
      error = 'Agent not found';
      loading = false;
      return;
    }

    loading = true;
    error = '';
    actionError = '';
    try {
      const client = await getGhostClient();
      const [agentData, overviewData] = await Promise.all([
        client.agents.get(agentId),
        client.agents.getOverview(agentId, { sessions_limit: 10, audit_limit: 20, crdt_limit: 50 }),
      ]);
      agent = agentData;
      overview = overviewData;
      syncEditableSandbox(agentData);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load agent details';
    } finally {
      loading = false;
    }
  }

  function syncEditableSandbox(agentData: AgentDetail) {
    editableSandbox = {
      enabled: agentData.sandbox?.enabled ?? true,
      mode: agentData.sandbox?.mode ?? 'workspace_write',
      on_violation: agentData.sandbox?.on_violation ?? 'pause',
      network_access: agentData.sandbox?.network_access ?? false,
      allowed_shell_prefixes: [...(agentData.sandbox?.allowed_shell_prefixes ?? [])],
    };
  }

  async function executeAction() {
    if (!agentId || !confirmAction) return;
    actionLoading = true;
    actionError = '';
    try {
      const client = await getGhostClient();
      if (confirmAction === 'pause') {
        await client.safety.pause(agentId, 'Paused from dashboard');
      } else if (confirmAction === 'quarantine') {
        await client.safety.quarantine(agentId, 'Quarantined from dashboard');
      } else if (confirmAction === 'resume_pause') {
        await client.safety.resume(agentId);
      } else {
        await client.safety.resume(agentId, {
          level: 'QUARANTINE',
          forensic_reviewed: forensicReviewed,
          second_confirmation: secondConfirmation,
        });
      }
      confirmAction = null;
      forensicReviewed = false;
      secondConfirmation = false;
      await loadData();
    } catch (e: unknown) {
      actionError = e instanceof Error ? e.message : 'Failed to execute action';
    } finally {
      actionLoading = false;
    }
  }

  async function saveSandbox() {
    if (!agentId) return;
    sandboxSaving = true;
    sandboxError = '';
    sandboxMessage = '';
    try {
      const client = await getGhostClient();
      await client.agents.update(agentId, {
        sandbox: editableSandbox,
      });
      sandboxMessage = 'Sandbox updated';
      await loadData();
    } catch (e: unknown) {
      sandboxError = e instanceof Error ? e.message : 'Failed to update sandbox';
    } finally {
      sandboxSaving = false;
    }
  }

  function statusColor(status: string | undefined): string {
    switch (status) {
      case 'ready': return 'var(--color-severity-normal)';
      case 'starting': return 'var(--color-severity-soft)';
      case 'paused': return 'var(--color-severity-soft)';
      case 'quarantined': return 'var(--color-severity-hard)';
      case 'kill_all_blocked': return 'var(--color-severity-active)';
      case 'stopping': return 'var(--color-severity-active)';
      case 'stopped': return 'var(--color-text-disabled)';
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

  function panelState(key: keyof NonNullable<AgentOverview>['panel_health']) {
    return overview?.panel_health?.[key] ?? { state: 'error', message: 'Overview unavailable' };
  }

  function isReady(key: keyof NonNullable<AgentOverview>['panel_health']) {
    return panelState(key).state === 'ready';
  }

  function renderPanelFallback(key: keyof NonNullable<AgentOverview>['panel_health'], emptyMessage: string): string {
    const panel = panelState(key);
    if (panel.state === 'empty') return emptyMessage;
    return panel.message ?? 'Panel unavailable';
  }

  function confirmTitle(action: ConfirmAction): string {
    switch (action) {
      case 'pause': return 'Pause Agent';
      case 'quarantine': return 'Quarantine Agent';
      case 'resume_pause': return 'Resume Agent';
      case 'resume_quarantine': return 'Resume Quarantined Agent';
      default: return 'Confirm';
    }
  }

  function confirmMessage(action: ConfirmAction): string {
    if (!agent) return 'Confirm action';
    switch (action) {
      case 'pause': return `Pause agent '${agent.name}'?`;
      case 'quarantine': return `Quarantine agent '${agent.name}'?`;
      case 'resume_pause': return `Resume agent '${agent.name}'?`;
      case 'resume_quarantine': return `Resume quarantined agent '${agent.name}' after review?`;
      default: return 'Confirm action';
    }
  }

  function closeReviewDialog() {
    confirmAction = null;
    actionError = '';
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeReviewDialog();
    }
  }
</script>

{#if loading}
  <div class="loading-state">Loading agent details...</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <a href="/agents" class="back-link">Back to agents</a>
  </div>
{:else if agent && overview}
  <div class="detail-header">
    <a href="/agents" class="back-link">Back to agents</a>
    <div class="header-row">
      <div>
        <h1>{agent.name}</h1>
        <p class="subtitle mono">{agent.id}</p>
      </div>
      <span class="status-badge" style="color: {statusColor(agent.effective_state ?? agent.status)}">
        {STATE_LABELS[agent.effective_state ?? agent.status] ?? agent.effective_state ?? agent.status}
      </span>
    </div>
  </div>

  <div class="controls">
    {#if agent.action_policy?.can_pause}
      <button class="btn btn-warning" onclick={() => confirmAction = 'pause'}>Pause</button>
    {/if}
    {#if agent.action_policy?.resume_kind === 'pause'}
      <button class="btn btn-success" onclick={() => confirmAction = 'resume_pause'}>Resume</button>
    {/if}
    {#if agent.action_policy?.resume_kind === 'quarantine'}
      <button class="btn btn-success" onclick={() => confirmAction = 'resume_quarantine'}>Resume After Review</button>
    {/if}
    {#if agent.action_policy?.can_quarantine}
      <button class="btn btn-danger" onclick={() => confirmAction = 'quarantine'}>Quarantine</button>
    {/if}
    {#if actionError}
      <span class="action-error">{actionError}</span>
    {/if}
  </div>

  <div class="detail-grid">
    <section class="card">
      <h2>Convergence</h2>
      {#if isReady('convergence') && overview.convergence}
        <div class="gauge-center">
          <ScoreGauge score={overview.convergence.score} level={overview.convergence.level} />
        </div>
        {#if overview.convergence.signal_scores && Object.keys(overview.convergence.signal_scores).length > 0}
          <div class="signal-list">
            {#each Object.entries(overview.convergence.signal_scores) as [key, value]}
              <div class="signal-row">
                <span class="signal-name">{signalLabels[key] ?? key}</span>
                <span class="signal-value">{typeof value === 'number' ? value.toFixed(2) : value}</span>
              </div>
            {/each}
          </div>
        {/if}
        {#if overview.convergence.computed_at}
          <p class="timestamp">Updated {new Date(overview.convergence.computed_at).toLocaleString()}</p>
        {/if}
      {:else}
        <p class="panel-fallback">{renderPanelFallback('convergence', 'No convergence data available')}</p>
      {/if}
    </section>

    <section class="card">
      <h2>Sandbox</h2>
      <div class="sandbox-summary">
        <div><strong>Isolation:</strong> {agent.isolation ?? 'in_process'}</div>
        <div><strong>Pending reviews:</strong> {agent.sandbox_metrics?.pending_reviews ?? 0}</div>
        <div><strong>Total reviews:</strong> {agent.sandbox_metrics?.total_reviews ?? 0}</div>
      </div>
      <div class="sandbox-form">
        <label class="checkbox-row">
          <input type="checkbox" bind:checked={editableSandbox.enabled} />
          <span>Enable builtin sandbox</span>
        </label>

        <label class="field-row">
          <span>Mode</span>
          <select bind:value={editableSandbox.mode}>
            <option value="off">Off</option>
            <option value="read_only">Read Only</option>
            <option value="workspace_write">Workspace Write</option>
            <option value="strict">Strict</option>
          </select>
        </label>

        <label class="field-row">
          <span>On Violation</span>
          <select bind:value={editableSandbox.on_violation}>
            <option value="warn">Warn</option>
            <option value="pause">Pause</option>
            <option value="quarantine">Quarantine</option>
            <option value="kill_all">Kill All</option>
          </select>
        </label>

        <label class="checkbox-row">
          <input type="checkbox" bind:checked={editableSandbox.network_access} />
          <span>Allow networked builtin tools</span>
        </label>

        <label class="field-row">
          <span>Shell Prefixes</span>
          <input
            type="text"
            value={editableSandbox.allowed_shell_prefixes.join(', ')}
            oninput={(event) => {
              const target = event.currentTarget as HTMLInputElement;
              editableSandbox.allowed_shell_prefixes = target.value.split(',').map((value) => value.trim()).filter(Boolean);
            }}
            placeholder="npm test, cargo test"
          />
        </label>

        <button class="btn btn-primary" onclick={saveSandbox} disabled={sandboxSaving}>
          {sandboxSaving ? 'Saving...' : 'Save Sandbox'}
        </button>

        {#if sandboxMessage}
          <p class="success-text">{sandboxMessage}</p>
        {/if}
        {#if sandboxError}
          <p class="action-error">{sandboxError}</p>
        {/if}
      </div>
    </section>

    <section class="card">
      <h2>Costs</h2>
      {#if isReady('cost') && overview.cost}
        <div class="cost-rows">
          <div class="cost-row"><span class="cost-label">Daily Total</span><span class="cost-value">${overview.cost.daily_total.toFixed(4)}</span></div>
          <div class="cost-row"><span class="cost-label">Compaction</span><span class="cost-value">${overview.cost.compaction_cost.toFixed(4)}</span></div>
          <div class="cost-row"><span class="cost-label">Cap</span><span class="cost-value">${overview.cost.spending_cap.toFixed(2)}</span></div>
          <div class="cost-row"><span class="cost-label">Remaining</span><span class="cost-value">${overview.cost.cap_remaining.toFixed(2)}</span></div>
        </div>
        <div class="utilization-bar">
          <CostBar utilization={overview.cost.cap_utilization_pct} />
        </div>
      {:else}
        <p class="panel-fallback">{renderPanelFallback('cost', 'No cost data available')}</p>
      {/if}
    </section>

    <section class="card">
      <h2>Recent Sessions</h2>
      {#if isReady('recent_sessions') && overview.recent_sessions.length > 0}
        <div class="session-list">
          {#each overview.recent_sessions as session}
            <div class="session-row">
              <span class="session-id" title={session.session_id}>{session.session_id.slice(0, 8)}...</span>
              <span class="session-events">{session.event_count} events</span>
              <span class="session-time">{new Date(session.started_at).toLocaleDateString()}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="panel-fallback">{renderPanelFallback('recent_sessions', 'No sessions found for this agent')}</p>
      {/if}
    </section>

    <section class="card">
      <h2>Audit Log</h2>
      {#if isReady('recent_audit_entries') && overview.recent_audit_entries.length > 0}
        <div class="audit-list">
          {#each overview.recent_audit_entries as entry}
            <div class="audit-row">
              <span class="audit-severity" style="color: {severityColor(entry.severity)}">{entry.severity}</span>
              <span class="audit-type">{entry.event_type}</span>
              <span class="audit-time">{new Date(entry.timestamp).toLocaleString()}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="panel-fallback">{renderPanelFallback('recent_audit_entries', 'No audit entries for this agent')}</p>
      {/if}
    </section>

    <section class="card">
      <h2>CRDT State</h2>
      {#if isReady('crdt_summary') && overview.crdt_summary && overview.crdt_summary.deltas.length > 0}
        <div class="crdt-meta">
          <span>{overview.crdt_summary.total} total deltas</span>
          <span class="chain-indicator" class:valid={overview.crdt_summary.chain_valid} class:broken={!overview.crdt_summary.chain_valid}>
            Chain: {overview.crdt_summary.chain_valid ? 'Valid' : 'Broken'}
          </span>
        </div>
        <div class="crdt-list">
          {#each overview.crdt_summary.deltas.slice(0, 20) as delta}
            <div class="crdt-row">
              <span class="crdt-type">{delta.event_type}</span>
              <span class="crdt-memory">{delta.memory_id?.slice(0, 8)}...</span>
              <span class="crdt-time">{new Date(delta.recorded_at).toLocaleString()}</span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="panel-fallback">{renderPanelFallback('crdt_summary', 'No CRDT deltas for this agent')}</p>
      {/if}
    </section>

    <section class="card wide">
      <h2>Hash Chain Integrity</h2>
      {#if isReady('integrity_summary') && overview.integrity_summary?.chains}
        {@const chains = overview.integrity_summary.chains as IntegrityChains}
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
        <p class="panel-fallback">{renderPanelFallback('integrity_summary', 'No integrity data available')}</p>
      {/if}
    </section>
  </div>

  {#if confirmAction === 'pause' || confirmAction === 'quarantine' || confirmAction === 'resume_pause'}
    <ConfirmDialog
      title={confirmTitle(confirmAction)}
      message={confirmMessage(confirmAction)}
      confirmLabel={confirmAction === 'quarantine' ? 'Quarantine' : 'Confirm'}
      danger={confirmAction === 'quarantine'}
      loading={actionLoading}
      onconfirm={executeAction}
      oncancel={() => { confirmAction = null; actionError = ''; }}
    />
  {/if}

  {#if confirmAction === 'resume_quarantine'}
    <div
      class="overlay"
      role="button"
      tabindex="0"
      aria-label="Close quarantine review dialog"
      onclick={closeReviewDialog}
      onkeydown={handleOverlayKeydown}
    >
      <div
        class="review-dialog"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
        onkeydown={(event) => event.stopPropagation()}
      >
        <h2>Resume Quarantined Agent</h2>
        <p class="review-copy">Quarantine resume requires explicit forensic review and second confirmation. Resuming enables 24 hours of heightened monitoring.</p>
        <label class="checkbox-row">
          <input type="checkbox" bind:checked={forensicReviewed} />
          <span>I reviewed the forensic evidence for this quarantine.</span>
        </label>
        <label class="checkbox-row">
          <input type="checkbox" bind:checked={secondConfirmation} />
          <span>I confirm this agent should resume despite prior quarantine.</span>
        </label>
        {#if actionError}
          <p class="action-error">{actionError}</p>
        {/if}
        <div class="actions">
          <button class="cancel-btn" onclick={closeReviewDialog}>Cancel</button>
          <button class="confirm-btn" onclick={executeAction} disabled={!forensicReviewed || !secondConfirmation || actionLoading}>
            {actionLoading ? 'Working...' : 'Resume With Monitoring'}
          </button>
        </div>
      </div>
    </div>
  {/if}
{/if}

<style>
  .loading-state, .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-brand-primary);
    text-decoration: none;
  }

  .back-link:hover { text-decoration: underline; }

  .detail-header { margin-bottom: var(--spacing-4); }
  .header-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--spacing-3);
    margin-top: var(--spacing-2);
  }
  .header-row h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin: 0;
  }
  .subtitle {
    margin-top: var(--spacing-1);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
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
    flex-wrap: wrap;
    margin-bottom: var(--spacing-6);
  }

  .btn {
    padding: var(--spacing-1) var(--spacing-3);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }
  .btn-warning { background: var(--color-severity-soft); color: var(--color-text-inverse); }
  .btn-success { background: var(--color-severity-normal); color: var(--color-text-inverse); }
  .btn-danger { background: var(--color-interactive-danger); color: var(--color-text-inverse); }
  .btn-primary { background: var(--color-interactive-primary); color: var(--color-interactive-primary-text); }
  .btn:disabled { opacity: 0.6; cursor: not-allowed; }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: var(--spacing-4);
  }

  .card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }
  .card.wide { grid-column: 1 / -1; }
  .card h2 {
    margin: 0 0 var(--spacing-3) 0;
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
  }

  .gauge-center { display: flex; justify-content: center; margin-bottom: var(--spacing-3); }
  .signal-list, .cost-rows, .session-list, .audit-list, .crdt-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }
  .signal-row, .cost-row, .session-row, .audit-row, .crdt-row, .integrity-meta {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-2);
    font-size: var(--font-size-sm);
  }
  .signal-name, .cost-label, .session-events, .audit-type, .crdt-type {
    color: var(--color-text-muted);
  }
  .panel-fallback {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
  .timestamp, .success-text {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin-top: var(--spacing-3);
  }
  .success-text { color: var(--color-severity-normal); }
  .action-error { color: var(--color-severity-hard); font-size: var(--font-size-sm); }
  .mono { font-family: var(--font-family-mono); }

  .sandbox-summary, .sandbox-form {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }
  .checkbox-row, .field-row {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
    font-size: var(--font-size-sm);
  }
  .checkbox-row { flex-direction: row; align-items: center; }
  .field-row input, .field-row select {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    padding: var(--spacing-2);
  }

  .chain-indicator.valid { color: var(--color-severity-normal); }
  .chain-indicator.broken { color: var(--color-severity-hard); }
  .integrity-section + .integrity-section { margin-top: var(--spacing-3); }

  .overlay {
    position: fixed;
    inset: 0;
    background: var(--color-bg-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }
  .review-dialog {
    width: min(520px, 92vw);
    background: var(--color-bg-elevated-3);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-lg);
    padding: var(--spacing-6);
    box-shadow: var(--shadow-elevated-3);
  }
  .review-dialog h2 {
    margin: 0 0 var(--spacing-2) 0;
    font-size: var(--font-size-lg);
  }
  .review-copy {
    color: var(--color-text-secondary);
    margin-bottom: var(--spacing-4);
    font-size: var(--font-size-sm);
    line-height: var(--line-height-normal);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--spacing-2);
    margin-top: var(--spacing-4);
  }
  .cancel-btn, .confirm-btn {
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .cancel-btn {
    background: transparent;
    border: 1px solid var(--color-border-default);
    color: var(--color-text-secondary);
  }
  .confirm-btn {
    border: none;
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
  }
  .confirm-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  @media (max-width: 720px) {
    .detail-grid { grid-template-columns: 1fr; }
    .header-row { flex-direction: column; align-items: flex-start; }
  }
</style>
