<script lang="ts">
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { tick } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import {
    buildAuditExportParams,
    buildAuditQueryParams,
    KILL_LEVEL_LABELS,
    normalizeKillLevel,
    SECURITY_AUDIT_EVENT_TYPES,
    SECURITY_SEVERITY_LEVELS,
    type SecurityFilterState,
  } from '$lib/security-contract';
  import { authSessionStore } from '$lib/stores/auth-session.svelte';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import AuditTimeline from '../../components/AuditTimeline.svelte';
  import FilterBar from '../../components/FilterBar.svelte';
  import type { Agent, AuditEntry, AuditExportParams, SafetyStatus, SandboxReview } from '@ghost/sdk';

  type AgentIntervention = NonNullable<SafetyStatus['per_agent']>[string];

  let killState: SafetyStatus | null = $state(null);
  let auditEntries: AuditEntry[] = $state([]);
  let agents: Array<{ id: string; name: string }> = $state([]);
  let sandboxReviews: SandboxReview[] = $state([]);
  let reviewActionId = $state('');
  let loading = $state(true);
  let killStateError = $state('');
  let auditError = $state('');
  let reviewError = $state('');
  let agentsError = $state('');
  let focusedAuditId = $state('');
  let mounted = $state(false);
  let lastUrlState = $state('');
  let activeFilters = $state<SecurityFilterState>({
    from: '',
    to: '',
    agentId: '',
    eventType: '',
    severities: [],
    query: '',
  });

  const LEVEL_COLORS = [
    'var(--color-severity-normal)',
    'var(--color-severity-soft)',
    'var(--color-severity-active)',
    'var(--color-severity-hard)',
    'var(--color-severity-external)',
  ];

  const filterConfig = $derived({
    timeRange: true,
    agentSelector: true,
    agents,
    eventType: true,
    eventTypes: [...SECURITY_AUDIT_EVENT_TYPES],
    severity: true,
    severityLevels: [...SECURITY_SEVERITY_LEVELS],
    search: true,
    searchPlaceholder: 'Search audit entries…',
  });

  const filterInitialState = $derived({
    query: ($page.url.searchParams.get('search') ?? '').trim(),
  });

  const interventionEntries = $derived.by<[string, AgentIntervention][]>(() => {
    const interventions = (killState?.per_agent ?? {}) as NonNullable<SafetyStatus['per_agent']>;
    return Object.entries(interventions) as [string, AgentIntervention][];
  });

  async function loadKillState() {
    try {
      const client = await getGhostClient();
      killState = await client.safety.status();
      killStateError = '';
    } catch (e: unknown) {
      killState = null;
      killStateError = e instanceof Error ? e.message : 'Failed to load kill switch status';
    }
  }

  async function loadSandboxReviews() {
    try {
      const client = await getGhostClient();
      const reviewData = await client.safety.listSandboxReviews({ limit: 25 });
      sandboxReviews = reviewData.reviews ?? [];
      reviewError = '';
    } catch (e: unknown) {
      sandboxReviews = [];
      reviewError = e instanceof Error ? e.message : 'Failed to load sandbox reviews';
    }
  }

  async function loadAgents() {
    try {
      const client = await getGhostClient();
      const agentData = await client.agents.list();
      agents = (agentData ?? []).map((a: Agent) => ({ id: a.id, name: a.name }));
      agentsError = '';
    } catch (e: unknown) {
      agents = [];
      agentsError = e instanceof Error ? e.message : 'Failed to load agents';
    }
  }

  async function loadAuditEntries(filters: SecurityFilterState) {
    try {
      const client = await getGhostClient();
      const data = await client.audit.query(buildAuditQueryParams(filters));
      auditEntries = data?.entries ?? [];
      auditError = '';
      await focusAudit();
    } catch (e: unknown) {
      auditEntries = [];
      auditError = e instanceof Error ? e.message : 'Filter query failed';
    }
  }

  async function refreshSecuritySurface() {
    await Promise.all([
      loadKillState(),
      loadSandboxReviews(),
      loadAuditEntries(activeFilters),
    ]);
  }

  onMount(() => {
    mounted = true;
    lastUrlState = $page.url.search;
    activeFilters = {
      ...activeFilters,
      query: ($page.url.searchParams.get('search') ?? '').trim(),
    };
    focusedAuditId = ($page.url.searchParams.get('focus') ?? '').trim();
    // Load initial data (fire-and-forget async).
    (async () => {
      await Promise.all([
        loadKillState(),
        loadAuditEntries(activeFilters),
        loadAgents(),
        loadSandboxReviews(),
      ]);
      loading = false;
    })();

    const unsub1 = wsStore.on('KillSwitchActivation', () => { void refreshSecuritySurface(); });
    const unsub2 = wsStore.on('InterventionChange', () => { void refreshSecuritySurface(); });
    const unsub3 = wsStore.on('SandboxReviewRequested', () => { void refreshSecuritySurface(); });
    const unsub4 = wsStore.on('SandboxReviewResolved', () => { void refreshSecuritySurface(); });
    const unsubResync = wsStore.onResync(() => { void refreshSecuritySurface(); });
    return () => { unsub1(); unsub2(); unsub3(); unsub4(); unsubResync(); };
  });

  $effect(() => {
    if (!mounted) return;
    const urlState = $page.url.search;
    if (urlState === lastUrlState) return;
    lastUrlState = urlState;
    const search = ($page.url.searchParams.get('search') ?? '').trim();
    focusedAuditId = ($page.url.searchParams.get('focus') ?? '').trim();
    void applyFilters({ ...activeFilters, query: search });
  });

  async function applyFilters(state: SecurityFilterState) {
    activeFilters = {
      from: state.from ?? '',
      to: state.to ?? '',
      agentId: state.agentId ?? '',
      eventType: state.eventType ?? '',
      severities: [...(state.severities ?? [])],
      query: state.query ?? '',
    };
    await loadAuditEntries(activeFilters);
  }

  async function focusAudit() {
    if (!focusedAuditId) return;
    await tick();
    const target = document.getElementById(`audit-${focusedAuditId}`);
    target?.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }

  function getPlatformLevel(): number {
    return normalizeKillLevel(killState?.platform_level);
  }

  async function killAll() {
    if (!confirm('Are you sure you want to trigger KILL_ALL? This will stop all agents.')) {
      return;
    }
    try {
      const client = await getGhostClient();
      await client.safety.killAll('Manual trigger from dashboard', 'dashboard_ui');
      await refreshSecuritySurface();
    } catch (e: unknown) {
      alert('Failed to trigger kill switch: ' + (e instanceof Error ? e.message : String(e)));
    }
  }

  async function exportAudit(format: NonNullable<AuditExportParams['format']>) {
    try {
      const client = await getGhostClient();
      const blob = await client.audit.exportBlob(buildAuditExportParams(activeFilters, format));
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `audit-export.${format}`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (e: unknown) {
      alert('Export failed: ' + (e instanceof Error ? e.message : String(e)));
    }
  }

  async function resolveReview(reviewId: string, decision: 'approve' | 'reject') {
    reviewActionId = reviewId;
    try {
      const client = await getGhostClient();
      if (decision === 'approve') {
        await client.safety.approveSandboxReview(reviewId);
      } else {
        await client.safety.rejectSandboxReview(reviewId);
      }
      await refreshSecuritySurface();
    } catch (e: unknown) {
      reviewError = e instanceof Error ? e.message : `Failed to ${decision} sandbox review`;
    }
    reviewActionId = '';
  }
</script>

<h1 class="page-title">Security</h1>

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else}
  {#if killStateError}
    <div class="error-state">
      <p>{killStateError}</p>
      <button onclick={() => loadKillState()}>Retry</button>
    </div>
  {:else if killState}
    <div class="kill-state">
      <div class="kill-info">
        <span class="kill-label">Kill Switch</span>
        <span
          class="kill-level"
          style="color: {LEVEL_COLORS[getPlatformLevel()]}"
        >
          L{getPlatformLevel()} — {KILL_LEVEL_LABELS[getPlatformLevel()] ?? 'Unknown'}
        </span>
        <div class="kill-meta">
          <span>Platform killed: {killState.platform_killed ? 'yes' : 'no'}</span>
          {#if killState.distributed_kill}
            <span>Distributed kill: {killState.distributed_kill.status}</span>
          {/if}
          {#if killState.convergence_protection}
            <span>
              Convergence: {killState.convergence_protection.execution_mode}
              ({killState.convergence_protection.agents.healthy} healthy,
              {killState.convergence_protection.agents.stale + killState.convergence_protection.agents.missing + killState.convergence_protection.agents.corrupted} degraded)
            </span>
          {/if}
        </div>
      </div>
      {#if authSessionStore.canTriggerKillAll}
        <button class="danger-btn" onclick={killAll}>KILL ALL</button>
      {/if}
    </div>

    {#if !authSessionStore.canTriggerKillAll}
      <p class="section-note">Platform-wide kill is restricted to superadmin sessions.</p>
    {/if}

    {#if interventionEntries.length > 0}
      <div class="intervention-list">
        {#each interventionEntries as [agentId, intervention]}
          <div class="intervention-card">
            <strong>{agentId}</strong>
            <span>{intervention.level}</span>
            {#if intervention.trigger}
              <span>{intervention.trigger}</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}

  <div class="section-header">
    <h2>Sandbox Reviews</h2>
    {#if reviewError}
      <span class="section-error-text">{reviewError}</span>
    {/if}
  </div>

  {#if reviewError}
    <div class="error-state">
      <p>{reviewError}</p>
      <button onclick={() => loadSandboxReviews()}>Retry</button>
    </div>
  {:else if sandboxReviews.length > 0}
    <div class="review-list">
      {#each sandboxReviews as review}
        <div class="review-card">
          <div class="review-head">
            <div class="review-title">
              <strong>{review.tool_name}</strong>
              <span class="review-agent">{review.agent_id}</span>
            </div>
            <span class={`review-status review-status-${review.status}`}>{review.status}</span>
          </div>
          <p>{review.violation_reason}</p>
          <div class="review-meta">
            <span>{review.sandbox_mode}</span>
            <span>{new Date(review.requested_at).toLocaleString()}</span>
          </div>
          {#if review.status === 'pending'}
            {#if authSessionStore.canReviewSandbox}
              <div class="review-actions">
                <button disabled={reviewActionId === review.id} onclick={() => resolveReview(review.id, 'approve')}>
                  {reviewActionId === review.id ? 'Working…' : 'Approve'}
                </button>
                <button class="danger-outline" disabled={reviewActionId === review.id} onclick={() => resolveReview(review.id, 'reject')}>
                  Reject
                </button>
              </div>
            {:else}
              <p class="section-note">Approval requires admin or operator with `safety_review`.</p>
            {/if}
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <div class="empty-audit">No sandbox reviews recorded.</div>
  {/if}

  <div class="section-header">
    <h2>Audit Log</h2>
    {#if auditError}
      <span class="section-error-text">{auditError}</span>
    {/if}
    <div class="export-buttons">
      <button onclick={() => exportAudit('json')}>JSON</button>
      <button onclick={() => exportAudit('csv')}>CSV</button>
      <button onclick={() => exportAudit('jsonl')}>JSONL</button>
    </div>
  </div>

  <FilterBar config={filterConfig} initialState={filterInitialState} onfilter={applyFilters} />

  {#if agentsError}
    <p class="section-note">{agentsError}</p>
  {/if}

  {#if auditError}
    <div class="error-state">
      <p>{auditError}</p>
      <button onclick={() => loadAuditEntries(activeFilters)}>Retry</button>
    </div>
  {:else if auditEntries.length > 0}
    <AuditTimeline entries={auditEntries} focusedEntryId={focusedAuditId} />
  {:else}
    <div class="empty-audit">No audit entries match the current filters.</div>
  {/if}
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  .kill-state {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
    margin-bottom: var(--spacing-6);
  }

  .kill-info {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .kill-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wider);
  }

  .kill-level {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-bold);
  }

  .kill-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-3);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .intervention-list {
    display: grid;
    gap: var(--spacing-3);
    margin-bottom: var(--spacing-6);
  }

  .intervention-card {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-3);
    align-items: center;
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-3);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }

  .danger-btn {
    background: var(--color-severity-hard-bg);
    color: var(--color-severity-hard);
    border: 1px solid var(--color-severity-hard);
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-weight: var(--font-weight-semibold);
    font-size: var(--font-size-sm);
  }

  .danger-btn:hover {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-4);
  }

  .section-header h2 {
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-secondary);
  }

  .section-error-text {
    color: var(--color-severity-hard);
    font-size: var(--font-size-xs);
  }

  .section-note {
    margin: 0 0 var(--spacing-4);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .export-buttons {
    display: flex;
    gap: var(--spacing-2);
  }

  .export-buttons button {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-3);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }

  .export-buttons button:hover {
    background: var(--color-surface-hover);
  }

  .review-list {
    display: grid;
    gap: var(--spacing-3);
    margin-bottom: var(--spacing-6);
  }

  .review-card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .review-head {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-3);
    align-items: center;
    margin-bottom: var(--spacing-2);
  }

  .review-title {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .review-agent,
  .review-meta {
    display: flex;
    gap: var(--spacing-3);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .review-status {
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    font-size: var(--font-size-xs);
  }

  .review-status-pending {
    color: var(--color-severity-soft);
  }

  .review-status-approved {
    color: var(--color-severity-normal);
  }

  .review-status-rejected,
  .review-status-expired {
    color: var(--color-severity-hard);
  }

  .review-actions {
    display: flex;
    gap: var(--spacing-2);
    margin-top: var(--spacing-3);
  }

  .danger-outline {
    border: 1px solid var(--color-severity-hard);
    color: var(--color-severity-hard);
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

  .empty-audit {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
</style>
