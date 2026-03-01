<script lang="ts">
  /**
   * Observability page — trace visualization for agent sessions.
   * Select a session, view OTel spans in waterfall format.
   *
   * Ref: T-3.8.2
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import TraceWaterfall from '../../components/TraceWaterfall.svelte';

  interface Session {
    session_id: string;
    agent_id: string;
    event_count: number;
  }

  interface SpanRecord {
    span_id: string;
    trace_id: string;
    parent_span_id: string | null;
    operation_name: string;
    start_time: string;
    end_time: string | null;
    attributes: Record<string, unknown>;
    status: string;
  }

  interface TraceResponse {
    session_id: string;
    traces: { trace_id: string; spans: SpanRecord[] }[];
    total_spans: number;
  }

  let sessions: Session[] = $state([]);
  let selectedSession: string | null = $state(null);
  let spans: SpanRecord[] = $state([]);
  let totalSpans = $state(0);
  let loading = $state(false);
  let error: string | null = $state(null);

  onMount(() => {
    loadSessions();

    // T-5.9.1: Wire TraceUpdate WS event to refresh traces if a session is selected.
    const unsub = wsStore.on('TraceUpdate', () => {
      if (selectedSession) loadTraces(selectedSession);
    });
    return () => unsub();
  });

  async function loadSessions() {
    try {
      const res = await api.get('/api/sessions?limit=50');
      // T-5.9.4: Standardize — API returns { sessions: [...] }.
      sessions = res?.sessions ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load sessions';
    }
  }

  async function loadTraces(sessionId: string) {
    selectedSession = sessionId;
    loading = true;
    error = null;
    try {
      const res: TraceResponse = await api.get(`/api/traces/${sessionId}`);
      spans = res.traces.flatMap(t => t.spans);
      totalSpans = res.total_spans;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load traces';
      spans = [];
    } finally {
      loading = false;
    }
  }
</script>

<svelte:head>
  <title>Observability | ADE</title>
</svelte:head>

<div class="observability-page">
  <header class="page-header">
    <h1>Observability</h1>
    <p class="subtitle">OTel trace visualization for agent sessions</p>
  </header>

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
                onclick={() => loadTraces(session.session_id)}
              >
                <span class="session-id mono">{session.session_id.slice(0, 8)}…</span>
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
        <p class="loading">Loading traces…</p>
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
    padding: var(--spacing-6);
    max-width: 1200px;
  }

  .page-header {
    margin-bottom: var(--spacing-6);
  }

  .page-header h1 {
    font-size: var(--font-size-2xl);
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .subtitle {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin-top: var(--spacing-1);
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
    transition: background 0.1s;
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

  .placeholder, .loading, .empty-hint {
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
</style>
