<script lang="ts">
  import { onMount } from 'svelte';
  import { api, BASE_URL } from '$lib/api';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import AuditTimeline from '../../components/AuditTimeline.svelte';
  import FilterBar from '../../components/FilterBar.svelte';

  // T-5.9.5: Replace `any` with proper types.
  interface KillState {
    platform_level?: string;
    platform_killed?: boolean;
    platform?: { level?: number };
    per_agent?: Record<string, { level: string; activated_at?: string; trigger?: string }>;
    activated_at?: string;
    trigger?: string;
    distributed_gate?: Record<string, unknown>;
  }

  interface AuditEntry {
    id: string;
    timestamp: string;
    agent_id: string;
    event_type: string;
    severity: string;
    details: string;
  }

  let killState: KillState | null = $state(null);
  let auditEntries: AuditEntry[] = $state([]);
  let agents: Array<{ id: string; name: string }> = $state([]);
  let loading = $state(true);
  let error = $state('');

  const LEVEL_LABELS = ['Normal', 'Soft', 'Active', 'Hard', 'External'];
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
    eventTypes: ['tool_call', 'gate_check', 'intervention', 'kill_switch', 'proposal', 'session', 'auth'],
    severity: true,
    search: true,
    searchPlaceholder: 'Search audit entries…',
  });

  async function refreshSafety() {
    try {
      killState = await api.get('/api/safety/status');
    } catch { /* non-fatal refresh */ }
  }

  onMount(() => {
    // Load initial data (fire-and-forget async).
    (async () => {
      try {
        const [safetyData, auditData, agentData] = await Promise.all([
          api.get('/api/safety/status'),
          api.get('/api/audit?page_size=50'),
          api.get('/api/agents').catch(() => []),
        ]);
        killState = safetyData;
        auditEntries = auditData?.entries ?? [];
        agents = (agentData ?? []).map((a: { id: string; name: string }) => ({ id: a.id, name: a.name }));
      } catch (e: unknown) {
        error = e instanceof Error ? e.message : 'Failed to load security data';
      }
      loading = false;
    })();

    // T-5.9.1: Wire KillSwitchActivation + InterventionChange to refresh safety state.
    const unsub1 = wsStore.on('KillSwitchActivation', () => { refreshSafety(); });
    const unsub2 = wsStore.on('InterventionChange', () => { refreshSafety(); });
    return () => { unsub1(); unsub2(); };
  });

  interface FilterState {
    agentId?: string;
    eventType?: string;
    severities?: string[];
    from?: string;
    to?: string;
    query?: string;
  }

  async function applyFilters(state: FilterState) {
    try {
      const params = new URLSearchParams();
      params.set('page_size', '50');
      if (state.agentId) params.set('agent_id', state.agentId);
      if (state.eventType) params.set('event_type', state.eventType);
      if (state.severities?.length) params.set('severity', state.severities.join(','));
      if (state.from) params.set('from', new Date(state.from).toISOString());
      if (state.to) params.set('to', new Date(state.to).toISOString());
      if (state.query) params.set('q', state.query);

      const data = await api.get(`/api/audit?${params.toString()}`);
      auditEntries = data?.entries ?? [];
    } catch (e: unknown) {
      // T-5.9.2: Show filter error instead of swallowing.
      error = e instanceof Error ? e.message : 'Filter query failed';
    }
  }

  function getPlatformLevel(): number {
    if (killState?.platform_level != null) {
      const n = Number(killState.platform_level);
      return Number.isFinite(n) ? n : 0;
    }
    return killState?.platform?.level ?? 0;
  }

  async function killAll() {
    if (!confirm('Are you sure you want to trigger KILL_ALL? This will stop all agents.')) {
      return;
    }
    try {
      await api.post('/api/safety/kill-all');
      killState = await api.get('/api/safety/status');
    } catch (e: unknown) {
      alert('Failed to trigger kill switch: ' + (e instanceof Error ? e.message : String(e)));
    }
  }

  async function exportAudit(format: string) {
    try {
      const blob = await fetch(
        `${BASE_URL}/api/audit/export?format=${format}`,
        { headers: { Authorization: `Bearer ${sessionStorage.getItem('ghost-token') ?? ''}` } }
      ).then(r => r.blob());
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
</script>

<h1 class="page-title">Security</h1>

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={() => location.reload()}>Retry</button>
  </div>
{:else}
  <div class="kill-state">
    <div class="kill-info">
      <span class="kill-label">Kill Switch</span>
      <span
        class="kill-level"
        style="color: {LEVEL_COLORS[getPlatformLevel()]}"
      >
        L{getPlatformLevel()} — {LEVEL_LABELS[getPlatformLevel()] ?? 'Unknown'}
      </span>
    </div>
    <button class="danger-btn" onclick={killAll}>KILL ALL</button>
  </div>

  <div class="section-header">
    <h2>Audit Log</h2>
    <div class="export-buttons">
      <button onclick={() => exportAudit('json')}>JSON</button>
      <button onclick={() => exportAudit('csv')}>CSV</button>
      <button onclick={() => exportAudit('jsonl')}>JSONL</button>
    </div>
  </div>

  <FilterBar config={filterConfig} onfilter={applyFilters} />

  {#if auditEntries.length > 0}
    <AuditTimeline entries={auditEntries} />
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
