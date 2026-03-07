<script lang="ts">
  /**
   * ITP event snapshot view backed by GET /api/itp/events.
   * The gateway does not currently expose live ITP websocket events or content payloads.
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';

  interface ItpEvent {
    id: string;
    event_type: string;
    platform: string;
    session_id: string;
    timestamp: string;
    source: string;
  }

  let events: ItpEvent[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let extensionConnected = $state(false);
  let bufferCount = $state(0);

  let logContainer = $state<HTMLDivElement | null>(null);

  function eventTypeColor(type: string): string {
    switch (type) {
      case 'SessionStart': return 'var(--color-severity-normal)';
      case 'SessionEnd': return 'var(--color-severity-soft)';
      case 'Interaction': return 'var(--color-interactive-primary)';
      case 'Error': return 'var(--color-severity-hard)';
      default: return 'var(--color-text-muted)';
    }
  }

  function formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString('en-US', { hour12: false });
  }

  async function loadEvents() {
    loading = true;
    error = '';

    try {
      const client = await getGhostClient();
      const data = await client.itp.list({ limit: 200 });
      events = data?.events ?? [];
      bufferCount = data?.buffer_count ?? 0;
      extensionConnected = data?.extension_connected ?? false;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load ITP events';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadEvents();
  });

  $effect(() => {
    if (events.length > 0 && logContainer) {
      requestAnimationFrame(() => {
        if (logContainer) {
          logContainer.scrollTop = logContainer.scrollHeight;
        }
      });
    }
  });
</script>

<div class="page-header">
  <div>
    <h1 class="page-title">ITP Events</h1>
    <p class="page-subtitle">Persisted gateway snapshot. Live streaming is not currently available for this route.</p>
  </div>
  <button class="refresh-btn" onclick={() => loadEvents()}>
    Refresh
  </button>
</div>

{#if error}
  <div class="error-banner" role="alert">
    <span>{error}</span>
    <button onclick={() => loadEvents()}>Retry</button>
  </div>
{/if}

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else}
  <div class="event-log" bind:this={logContainer}>
    {#if events.length === 0}
      <div class="empty-log">No ITP events are currently stored.</div>
    {:else}
      {#each events as event (event.id)}
        <div class="event-row">
          <span class="event-time">{formatTime(event.timestamp)}</span>
          <span class="event-type" style="color: {eventTypeColor(event.event_type)}">{event.event_type}</span>
          <span class="event-platform">{event.platform}</span>
          <span class="event-session" title={event.session_id}>{event.session_id.slice(0, 10)}</span>
          <span class="event-source">{event.source}</span>
        </div>
      {/each}
    {/if}
  </div>

  <div class="status-bar">
    <span>Buffer: {bufferCount} events</span>
    <span>Source: persisted gateway buffer</span>
    <span class="ext-status" class:ext-connected={extensionConnected}>
      Monitor link: {extensionConnected ? 'Connected' : 'Unavailable'}
    </span>
  </div>
{/if}

<style>
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
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

  .refresh-btn {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .event-log {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md) var(--radius-md) 0 0;
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    max-height: 500px;
    overflow-y: auto;
    padding: var(--spacing-2);
  }

  .event-row {
    display: grid;
    grid-template-columns: 80px 110px 80px 90px 1fr;
    gap: var(--spacing-2);
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: var(--radius-xs, 2px);
  }

  .event-row:hover {
    background: var(--color-surface-hover);
  }

  .event-time {
    color: var(--color-text-muted);
  }

  .event-type {
    font-weight: var(--font-weight-semibold);
  }

  .event-platform {
    color: var(--color-text-secondary, var(--color-text-muted));
  }

  .event-session,
  .event-source {
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty-log {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
    font-family: var(--font-family-sans, inherit);
  }

  .status-bar {
    display: flex;
    gap: var(--spacing-4);
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-top: none;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .ext-status {
    color: var(--color-severity-hard);
  }

  .ext-status.ext-connected {
    color: var(--color-severity-normal);
  }

  .error-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-severity-hard-bg, rgba(255, 0, 0, 0.1));
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
    font-size: var(--font-size-sm);
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
    height: 400px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }
</style>
