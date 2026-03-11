<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import type {
    RuntimeSession,
    SessionTrace,
    TraceSpanRecord,
  } from '@ghost/sdk';
  import TraceWaterfall from '../../../components/TraceWaterfall.svelte';

  let sessions: RuntimeSession[] = $state([]);
  let selectedSession: string | null = $state(null);
  let spans: TraceSpanRecord[] = $state([]);
  let totalSpans = $state(0);
  let loading = $state(false);
  let error: string | null = $state(null);

  onMount(() => {
    void loadSessions();

    const unsub = wsStore.on('TraceUpdate', () => {
      if (selectedSession) void loadTraces(selectedSession);
    });
    const unsubResync = wsStore.onResync(() => {
      void loadSessions();
      if (selectedSession) void loadTraces(selectedSession);
    });
    return () => {
      unsub();
      unsubResync();
    };
  });

  async function loadSessions() {
    try {
      const client = await getGhostClient();
      const res = await client.runtimeSessions.list({ limit: 50 });
      sessions = getSessions(res);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load sessions';
    }
  }

  async function loadTraces(sessionId: string) {
    selectedSession = sessionId;
    loading = true;
    error = null;
    try {
      const client = await getGhostClient();
      const res: SessionTrace = await client.traces.get(sessionId);
      spans = res.traces.flatMap((trace) => trace.spans);
      totalSpans = res.total_spans;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load traces';
      spans = [];
    } finally {
      loading = false;
    }
  }

  function getSessions(data: { data: RuntimeSession[] }): RuntimeSession[] {
    return data.data;
  }
</script>

<svelte:head>
  <title>Trace Observability | ADE</title>
</svelte:head>

<div class="observability-page">
  <div class="content-layout">
    <aside class="session-selector">
      <h2>Sessions</h2>
      {#if sessions.length === 0}
        <p class="empty-hint">No sessions found</p>
      {:else}
        <ul class="session-list">
          {#each sessions as session}
            <li>
              <button
                class="session-btn"
                class:active={selectedSession === session.session_id}
                onclick={() => void loadTraces(session.session_id)}
              >
                <span class="session-id mono">{session.session_id.slice(0, 8)}...</span>
                <span class="session-meta">{session.event_count ?? 0} events</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>

    <main class="trace-area">
      {#if error}
        <p class="error-msg">{error}</p>
      {/if}

      {#if loading}
        <p class="loading">Loading traces...</p>
      {:else if selectedSession}
        <div class="trace-header">
          <span class="mono">{selectedSession}</span>
          <span class="span-count">{totalSpans} spans</span>
        </div>
        <TraceWaterfall {spans} />
      {:else}
        <p class="placeholder">Select a session to view its trace waterfall.</p>
      {/if}
    </main>
  </div>
</div>

<style>
  .observability-page {
    max-width: 1200px;
  }

  .content-layout {
    display: grid;
    grid-template-columns: 240px 1fr;
    gap: var(--spacing-4);
  }

  .session-selector {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
  }

  .session-selector h2 {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--color-text-secondary);
    margin-bottom: var(--spacing-2);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .session-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .session-btn {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    cursor: pointer;
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    transition: background var(--duration-fast) var(--easing-default);
  }

  .session-btn:hover {
    background: var(--color-bg-elevated-2);
  }

  .session-btn.active {
    background: var(--color-bg-elevated-2);
    border-color: var(--color-interactive-primary);
  }

  .session-id {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .session-meta {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .trace-area {
    min-height: 400px;
  }

  .trace-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-3);
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
  }

  .span-count {
    font-variant-numeric: tabular-nums;
  }

  .mono {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .placeholder,
  .loading,
  .empty-hint {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    text-align: center;
    padding: var(--spacing-8);
  }

  .error-msg {
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    margin-bottom: var(--spacing-3);
  }

  @media (max-width: 900px) {
    .content-layout {
      grid-template-columns: 1fr;
    }
  }
</style>
