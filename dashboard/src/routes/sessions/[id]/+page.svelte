<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import type { RuntimeSession, SessionEvent } from '@ghost/sdk';
  import HashChainStrip from '../../../components/HashChainStrip.svelte';

  let sessionId = $derived($page.params.id ?? '');
  let session: RuntimeSession | null = $state(null);
  let bookmarkCount = $state(0);
  let events: SessionEvent[] = $state([]);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state('');
  let chainValid = $state(true);
  let cumulativeCost = $state(0);
  let total = $state(0);
  let nextAfterSequenceNumber = $state<number | null>(null);
  let hasMoreEvents = $state(false);

  async function loadSession(resetEvents = true) {
    if (!sessionId) {
      return;
    }

    if (resetEvents) {
      loading = true;
    }
    error = '';

    try {
      const client = await getGhostClient();
      const [detail, eventPage] = await Promise.all([
        client.runtimeSessions.get(sessionId),
        client.runtimeSessions.events(sessionId, { limit: 200 }),
      ]);

      session = detail.session;
      bookmarkCount = detail.bookmark_count;
      events = eventPage.events ?? [];
      chainValid = eventPage.chain_valid ?? detail.session.chain_valid;
      cumulativeCost = eventPage.cumulative_cost ?? detail.session.cumulative_cost;
      total = eventPage.total ?? detail.session.event_count;
      nextAfterSequenceNumber = eventPage.next_after_sequence_number ?? null;
      hasMoreEvents = eventPage.has_more ?? false;
    } catch (errorValue: unknown) {
      error = errorValue instanceof Error ? errorValue.message : 'Failed to load session';
      session = null;
      bookmarkCount = 0;
      events = [];
      total = 0;
      chainValid = true;
      cumulativeCost = 0;
      nextAfterSequenceNumber = null;
      hasMoreEvents = false;
    } finally {
      loading = false;
    }
  }

  async function loadMoreEvents() {
    if (!sessionId || !hasMoreEvents || nextAfterSequenceNumber == null || loadingMore) {
      return;
    }

    loadingMore = true;
    try {
      const client = await getGhostClient();
      const eventPage = await client.runtimeSessions.events(sessionId, {
        after_sequence_number: nextAfterSequenceNumber,
        limit: 200,
      });
      events = [...events, ...(eventPage.events ?? [])];
      nextAfterSequenceNumber = eventPage.next_after_sequence_number ?? null;
      hasMoreEvents = eventPage.has_more ?? false;
      total = eventPage.total ?? total;
      chainValid = eventPage.chain_valid ?? chainValid;
      cumulativeCost = eventPage.cumulative_cost ?? cumulativeCost;
    } catch (errorValue: unknown) {
      error = errorValue instanceof Error ? errorValue.message : 'Failed to load more events';
    } finally {
      loadingMore = false;
    }
  }

  onMount(() => {
    void loadSession(true);
    const unsubSession = wsStore.on('SessionEvent', (msg) => {
      if (msg.session_id === sessionId) {
        void loadSession(true);
      }
    });
    const unsubCost = wsStore.on('CostUpdate', (msg) => {
      if (msg.session_id === sessionId) {
        void loadSession(true);
      }
    });
    const unsubResync = wsStore.onResync(() => {
      void loadSession(true);
    });
    return () => {
      unsubSession();
      unsubCost();
      unsubResync();
    };
  });

  let chainBlocks = $derived(
    events.map((event, index) => ({
      event_hash: event.event_hash ?? '',
      previous_hash: event.previous_hash ?? '',
      event_id: event.id,
      position: index,
    })),
  );
</script>

{#if loading}
  <div class="loading">Loading session…</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <a href="/sessions">← Back to Sessions</a>
  </div>
{:else if session}
  <div class="detail-header">
    <a href="/sessions" class="back-link">← Sessions</a>
    <h1>Session {session.session_id.slice(0, 12)}…</h1>
    <div class="header-meta">
      <span>{total} events</span>
      <span>{bookmarkCount} bookmarks</span>
      <span>Cost: ${cumulativeCost.toFixed(4)}</span>
      <span class:valid={chainValid} class:broken={!chainValid} class="chain-status">
        Chain: {chainValid ? 'Valid' : 'Broken'}
      </span>
      <a href={`/sessions/${session.session_id}/replay`} class="replay-link">Replay →</a>
    </div>
  </div>

  <section class="card">
    <h2>Session Summary</h2>
    <div class="summary-grid">
      <div>
        <span class="label">Agents</span>
        <strong>{session.agent_ids.length > 0 ? session.agent_ids.join(', ') : 'Unknown'}</strong>
      </div>
      <div>
        <span class="label">Started</span>
        <strong>{new Date(session.started_at).toLocaleString()}</strong>
      </div>
      <div>
        <span class="label">Last Event</span>
        <strong>{new Date(session.last_event_at).toLocaleString()}</strong>
      </div>
      <div>
        <span class="label">Branched From</span>
        <strong>{session.branched_from ?? 'Original session'}</strong>
      </div>
    </div>
  </section>

  <section class="card">
    <h2>Hash Chain</h2>
    <HashChainStrip blocks={chainBlocks} maxVisible={30} />
  </section>

  <section class="card">
    <h2>Events ({events.length} of {total})</h2>
    <div class="events-table-wrap">
      <table>
        <thead>
          <tr>
            <th>#</th>
            <th>Type</th>
            <th>Sender</th>
            <th>Timestamp</th>
            <th>Tokens</th>
            <th>Latency</th>
          </tr>
        </thead>
        <tbody>
          {#each events as event}
            <tr>
              <td class="mono">{event.sequence_number}</td>
              <td>{event.event_type}</td>
              <td>{event.sender ?? '—'}</td>
              <td class="timestamp">{new Date(event.timestamp).toLocaleString()}</td>
              <td class="mono">{event.token_count ?? '—'}</td>
              <td class="mono">{event.latency_ms != null ? `${event.latency_ms}ms` : '—'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    {#if hasMoreEvents}
      <div class="load-more">
        <button onclick={loadMoreEvents} disabled={loadingMore}>
          {loadingMore ? 'Loading…' : 'Load More Events'}
        </button>
      </div>
    {/if}
  </section>
{/if}

<style>
  .loading,
  .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .detail-header {
    margin-bottom: var(--spacing-4);
  }

  .detail-header h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin: var(--spacing-2) 0;
  }

  .header-meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-4);
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    align-items: center;
  }

  .chain-status.valid {
    color: var(--color-severity-normal);
  }

  .chain-status.broken {
    color: var(--color-severity-hard);
  }

  .replay-link {
    color: var(--color-interactive-primary);
    text-decoration: none;
    font-weight: var(--font-weight-medium);
  }

  .card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
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

  .summary-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: var(--spacing-3);
  }

  .label {
    display: block;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-1);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .events-table-wrap {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    text-align: left;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    padding: var(--spacing-2);
    border-bottom: 1px solid var(--color-border-default);
  }

  td {
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .mono {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .timestamp {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .load-more {
    margin-top: var(--spacing-3);
    text-align: center;
  }

  .load-more button {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
  }
</style>
