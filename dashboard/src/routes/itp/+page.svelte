<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import type { ItpEvent, ListItpEventsResult } from '@ghost/sdk';

  let events: ItpEvent[] = $state([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state('');
  let monitorConnected = $state(false);
  let liveUpdatesSupported = $state(false);
  let totalPersisted = $state(0);
  let totalFiltered = $state(0);
  let returned = $state(0);
  let limit = $state(100);
  let offset = $state(0);
  let sessionFilter = $state('');
  let eventTypeFilter = $state('');

  let refreshInterval: ReturnType<typeof setInterval> | null = null;
  let refreshDebounce: ReturnType<typeof setTimeout> | null = null;
  let unsubs: Array<() => void> = [];

  const wsState = $derived(wsStore.state);

  async function loadEvents(options?: { silent?: boolean }) {
    const silent = options?.silent ?? false;
    if (silent) {
      refreshing = true;
    } else {
      loading = true;
    }
    error = '';

    try {
      const client = await getGhostClient();
      const data = await client.itp.list({
        limit,
        offset,
        session_id: sessionFilter.trim() || undefined,
        event_type: eventTypeFilter.trim() || undefined,
      });
      applyResponse(data);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load ITP events';
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  function applyResponse(data: ListItpEventsResult) {
    events = data?.events ?? [];
    monitorConnected = data?.monitor_connected ?? false;
    liveUpdatesSupported = data?.live_updates_supported ?? false;
    totalPersisted = data?.total_persisted ?? 0;
    totalFiltered = data?.total_filtered ?? 0;
    returned = data?.returned ?? 0;
  }

  function scheduleRefresh() {
    if (refreshDebounce) {
      clearTimeout(refreshDebounce);
    }
    refreshDebounce = setTimeout(() => {
      void loadEvents({ silent: true });
    }, 250);
  }

  function applyFilters() {
    offset = 0;
    void loadEvents();
  }

  function previousPage() {
    offset = Math.max(0, offset - limit);
    void loadEvents({ silent: true });
  }

  function nextPage() {
    if (offset + limit >= totalFiltered) {
      return;
    }
    offset += limit;
    void loadEvents({ silent: true });
  }

  function formatTime(ts: string): string {
    return new Date(ts).toLocaleString();
  }

  function platformRouteLabel(event: ItpEvent): string {
    if (event.platform && event.route) {
      return `${event.platform} / ${event.route}`;
    }
    return event.platform ?? event.route ?? '—';
  }

  function wsStatusLabel(state: string): string {
    switch (state) {
      case 'connected':
        return 'Connected';
      case 'reconnecting':
        return 'Reconnecting';
      case 'follower':
        return 'Connected (Follower)';
      case 'connecting':
        return 'Connecting';
      default:
        return 'Disconnected';
    }
  }

  onMount(() => {
    void loadEvents();

    unsubs = [
      wsStore.on('SessionEvent', (msg) => {
        const incomingSessionId = msg.session_id as string | undefined;
        if (sessionFilter && incomingSessionId && incomingSessionId !== sessionFilter.trim()) {
          return;
        }
        scheduleRefresh();
      }),
      wsStore.onResync(() => {
        void loadEvents({ silent: true });
      }),
    ];

    // Periodic reconciliation prevents silent drift when an upstream producer
    // persists rows without a matching websocket notification.
    refreshInterval = setInterval(() => {
      void loadEvents({ silent: true });
    }, 15_000);

    return () => {
      for (const unsub of unsubs) {
        unsub();
      }
      if (refreshInterval) {
        clearInterval(refreshInterval);
      }
      if (refreshDebounce) {
        clearTimeout(refreshDebounce);
      }
    };
  });

</script>

<div class="page-header">
  <div>
    <h1 class="page-title">ITP Events</h1>
    <p class="page-subtitle">Auto-refreshing event view with websocket signals, reconciliation polling, and direct session drilldown.</p>
  </div>
  <button class="refresh-btn" onclick={() => loadEvents({ silent: true })} disabled={loading || refreshing}>
    {refreshing ? 'Refreshing…' : 'Refresh'}
  </button>
</div>

<div class="status-grid">
  <div class="status-card">
    <span class="status-label">WebSocket</span>
    <strong>{wsStatusLabel(wsState)}</strong>
  </div>
  <div class="status-card">
    <span class="status-label">Monitor</span>
    <strong class:healthy={monitorConnected} class:degraded={!monitorConnected}>
      {monitorConnected ? 'Connected' : 'Unavailable'}
    </strong>
  </div>
  <div class="status-card">
    <span class="status-label">Auto Refresh</span>
    <strong>{liveUpdatesSupported ? 'WS + Reconcile' : 'Snapshot Only'}</strong>
  </div>
  <div class="status-card">
    <span class="status-label">Persisted</span>
    <strong>{totalPersisted}</strong>
  </div>
  <div class="status-card">
    <span class="status-label">Filtered</span>
    <strong>{totalFiltered}</strong>
  </div>
  <div class="status-card">
    <span class="status-label">Returned</span>
    <strong>{returned}</strong>
  </div>
</div>

<form class="filters" onsubmit={(event) => { event.preventDefault(); applyFilters(); }}>
  <label>
    <span>Session</span>
    <input bind:value={sessionFilter} placeholder="session id" />
  </label>
  <label>
    <span>Event Type</span>
    <input bind:value={eventTypeFilter} placeholder="tool_use, turn_complete…" />
  </label>
  <button class="apply-btn" type="submit">Apply</button>
</form>

<div class="pagination-bar">
  <div class="pagination-meta">
    {#if totalFiltered === 0}
      <span>0 results</span>
    {:else}
      <span>{offset + 1}-{Math.min(offset + returned, totalFiltered)} of {totalFiltered}</span>
    {/if}
  </div>
  <div class="pagination-actions">
    <button class="apply-btn" type="button" onclick={previousPage} disabled={loading || refreshing || offset === 0}>
      Previous
    </button>
    <button
      class="apply-btn"
      type="button"
      onclick={nextPage}
      disabled={loading || refreshing || offset + limit >= totalFiltered}
    >
      Next
    </button>
  </div>
</div>

{#if error}
  <div class="error-banner" role="alert">
    <span>{error}</span>
    <button onclick={() => loadEvents()}>Retry</button>
  </div>
{/if}

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else if events.length === 0}
  <div class="empty-state">
    <p>No ITP events match the current filters.</p>
  </div>
{:else}
  <div class="table-wrap">
    <table>
      <thead>
        <tr>
          <th>Time</th>
          <th>Type</th>
          <th>Sender</th>
          <th>Source</th>
          <th>Platform / Route</th>
          <th>Session</th>
          <th>Seq</th>
          <th>Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each events as event (event.id)}
          <tr class="event-row">
            <td class="timestamp">{formatTime(event.timestamp)}</td>
            <td class="mono">{event.event_type}</td>
            <td>{event.sender ?? '—'}</td>
            <td>{event.source ?? '—'}</td>
            <td>{platformRouteLabel(event)}</td>
            <td class="mono">
              <a href={event.session_path} title={event.session_id}>
                {event.session_id.slice(0, 12)}…
              </a>
            </td>
            <td class="mono">{event.sequence_number}</td>
            <td class="actions">
              <a href={event.session_path}>Detail</a>
              <a href={event.replay_path}>Replay</a>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<style>
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
  }

  .page-subtitle {
    margin-top: var(--spacing-1);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .refresh-btn,
  .apply-btn {
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .refresh-btn:disabled {
    cursor: wait;
    opacity: 0.7;
  }

  .status-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: var(--spacing-3);
    margin-bottom: var(--spacing-4);
  }

  .status-card,
  .filters,
  .table-wrap,
  .empty-state,
  .skeleton-block {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .status-card {
    padding: var(--spacing-3);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .status-label {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .healthy {
    color: var(--color-severity-normal);
  }

  .degraded {
    color: var(--color-severity-hard);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-3);
    align-items: end;
    padding: var(--spacing-3);
    margin-bottom: var(--spacing-4);
  }

  .pagination-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--spacing-3);
    margin-bottom: var(--spacing-4);
  }

  .pagination-meta {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .pagination-actions {
    display: flex;
    gap: var(--spacing-2);
  }

  .filters label {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
    min-width: 220px;
  }

  .filters span {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .filters input {
    padding: var(--spacing-2);
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
  }

  .table-wrap {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    padding: var(--spacing-3);
    text-align: left;
    border-bottom: 1px solid var(--color-border-subtle);
    font-size: var(--font-size-sm);
  }

  th {
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    font-size: var(--font-size-xs);
  }

  .mono {
    font-family: var(--font-family-mono);
  }

  .timestamp {
    white-space: nowrap;
    color: var(--color-text-muted);
  }

  .actions {
    display: flex;
    gap: var(--spacing-2);
    white-space: nowrap;
  }

  .actions a,
  td a {
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .actions a:hover,
  td a:hover {
    text-decoration: underline;
  }

  .empty-state {
    padding: var(--spacing-8);
    text-align: center;
    color: var(--color-text-muted);
  }

  .error-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--spacing-3);
    padding: var(--spacing-3);
    background: var(--color-severity-hard-bg, rgba(255, 0, 0, 0.08));
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
    color: var(--color-severity-hard);
  }

  .error-banner button {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    text-decoration: underline;
  }

  .skeleton-block {
    height: 360px;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.45; }
    50% { opacity: 0.75; }
  }
</style>
