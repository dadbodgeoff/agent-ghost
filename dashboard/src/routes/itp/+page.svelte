<script lang="ts">
  /**
   * ITP Event Viewer (Phase 3, Task 3.3).
   * Live tail of ITP events from the browser extension and other sources.
   * Privacy-level masking for content display.
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';

  interface ItpEvent {
    id: string;
    event_type: string;
    platform: string;
    session_id: string;
    content?: string;
    timestamp: string;
    source: string;
  }

  type PrivacyLevel = 'minimal' | 'standard' | 'full';

  let events: ItpEvent[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let paused = $state(false);
  let privacyLevel: PrivacyLevel = $state('minimal');
  let extensionConnected = $state(false);
  let bufferCount = $state(0);
  let autoScroll = $state(true);

  let logContainer = $state<HTMLDivElement | null>(null);

  function maskContent(content: string | undefined, level: PrivacyLevel): string {
    if (!content) return '—';
    switch (level) {
      case 'minimal':
        return '****';
      case 'standard': {
        if (content.length <= 20) return '****';
        return content.slice(0, 10) + '…' + content.slice(-10);
      }
      case 'full':
        return content;
    }
  }

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
    try {
      const client = await getGhostClient();
      const data = await client.itp.list({ limit: 200 });
      events = data?.events ?? [];
      bufferCount = data?.buffer_count ?? 0;
      extensionConnected = data?.extension_connected ?? false;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load ITP events';
    }
    loading = false;
  }

  function scrollToBottom() {
    if (logContainer && autoScroll && !paused) {
      logContainer.scrollTop = logContainer.scrollHeight;
    }
  }

  function handleNewEvent(event: unknown) {
    if (paused) return;
    const evt = event as ItpEvent;
    events = [...events.slice(-499), evt];
    bufferCount++;
    requestAnimationFrame(scrollToBottom);
  }

  onMount(() => {
    loadEvents();
    const unsub = wsStore.on('ItpEvent', handleNewEvent);
    return () => unsub();
  });

  $effect(() => {
    // Auto-scroll when events change
    if (events.length > 0) {
      requestAnimationFrame(scrollToBottom);
    }
  });
</script>

<div class="page-header">
  <h1 class="page-title">ITP Event Stream</h1>
  <button
    class="pause-btn"
    class:paused
    onclick={() => { paused = !paused; if (!paused) autoScroll = true; }}
  >
    {paused ? 'Resume' : 'Pause'}
  </button>
</div>

{#if error}
  <div class="error-banner" role="alert">
    <span>{error}</span>
    <button onclick={() => { error = ''; loadEvents(); }}>Retry</button>
  </div>
{/if}

<div class="controls-bar">
  <label class="privacy-selector">
    <span class="label-text">Privacy Level:</span>
    <select bind:value={privacyLevel}>
      <option value="minimal">Minimal</option>
      <option value="standard">Standard</option>
      <option value="full">Full</option>
    </select>
  </label>
</div>

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else}
  <div class="event-log" bind:this={logContainer}>
    {#if events.length === 0}
      <div class="empty-log">No ITP events yet. Events appear when the browser extension detects AI interactions.</div>
    {:else}
      {#each events as event (event.id)}
        <div class="event-row">
          <span class="event-time">{formatTime(event.timestamp)}</span>
          <span class="event-type" style="color: {eventTypeColor(event.event_type)}">{event.event_type}</span>
          <span class="event-platform">{event.platform}</span>
          <span class="event-session" title={event.session_id}>{event.session_id.slice(0, 10)}</span>
          <span class="event-content">{maskContent(event.content, privacyLevel)}</span>
        </div>
      {/each}
    {/if}
  </div>

  <div class="status-bar">
    <span>Buffer: {bufferCount} events</span>
    <span>Source: gateway stream</span>
    <span class="ext-status" class:ext-connected={extensionConnected}>
      Monitor link: {extensionConnected ? 'Connected' : 'Unavailable'}
    </span>
    {#if paused}
      <span class="paused-indicator">Paused</span>
    {/if}
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

  .pause-btn {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .pause-btn.paused {
    background: var(--color-severity-soft);
    color: var(--color-text-inverse);
    border-color: var(--color-severity-soft);
  }

  .controls-bar {
    display: flex;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .privacy-selector {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
  }

  .label-text {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .privacy-selector select {
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
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

  .event-session {
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .event-content {
    color: var(--color-text-primary);
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

  .paused-indicator {
    margin-left: auto;
    color: var(--color-severity-soft);
    font-weight: var(--font-weight-semibold);
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
