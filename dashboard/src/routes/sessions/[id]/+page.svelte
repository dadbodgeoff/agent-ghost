<script lang="ts">
  /**
   * Session detail page (T-2.5.1).
   * Shows session events, hash chain status, and links to replay.
   */
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api';
  import HashChainStrip from '../../../components/HashChainStrip.svelte';

  let sessionId = $derived($page.params.id ?? '');
  let events: any[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let chainValid = $state(true);
  let cumulativeCost = $state(0);
  let total = $state(0);

  onMount(async () => {
    try {
      const data = await api.get(`/api/sessions/${sessionId}/events?limit=200`);
      events = data?.events ?? [];
      chainValid = data?.chain_valid ?? true;
      cumulativeCost = data?.cumulative_cost ?? 0;
      total = data?.total ?? 0;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load session';
    }
    loading = false;
  });

  let chainBlocks = $derived(events.map((e, i) => ({
    event_hash: e.event_hash ?? '',
    previous_hash: e.previous_hash ?? '',
    event_id: e.id,
    position: i,
  })));
</script>

{#if loading}
  <div class="loading">Loading session…</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <a href="/sessions">← Back to Sessions</a>
  </div>
{:else}
  <div class="detail-header">
    <a href="/sessions" class="back-link">← Sessions</a>
    <h1>Session {sessionId.slice(0, 12)}…</h1>
    <div class="header-meta">
      <span>{total} events</span>
      <span>Cost: ${cumulativeCost.toFixed(4)}</span>
      <span class="chain-status" class:valid={chainValid} class:broken={!chainValid}>
        Chain: {chainValid ? 'Valid' : 'Broken'}
      </span>
      <a href="/sessions/{sessionId}/replay" class="replay-link">Replay →</a>
    </div>
  </div>

  <!-- Hash Chain Visualization -->
  <section class="card">
    <h2>Hash Chain</h2>
    <HashChainStrip blocks={chainBlocks} maxVisible={30} />
  </section>

  <!-- Events Table -->
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
          {#each events as event, i}
            <tr>
              <td class="mono">{event.sequence_number}</td>
              <td>{event.event_type}</td>
              <td>{event.sender ?? '—'}</td>
              <td class="timestamp">{new Date(event.timestamp).toLocaleString()}</td>
              <td class="mono">{event.token_count ?? '—'}</td>
              <td class="mono">{event.latency_ms != null ? event.latency_ms + 'ms' : '—'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>
{/if}

<style>
  .loading, .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .detail-header { margin-bottom: var(--spacing-4); }

  .detail-header h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin: var(--spacing-2) 0;
  }

  .header-meta {
    display: flex;
    gap: var(--spacing-4);
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    align-items: center;
  }

  .chain-status.valid { color: var(--color-severity-normal); }
  .chain-status.broken { color: var(--color-severity-hard); }

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

  .events-table-wrap { overflow-x: auto; }

  table { width: 100%; border-collapse: collapse; }

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

  .mono { font-family: var(--font-family-mono); font-size: var(--font-size-xs); }
  .timestamp { font-size: var(--font-size-xs); color: var(--color-text-muted); }
</style>
