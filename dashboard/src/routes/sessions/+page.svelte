<script lang="ts">
  import { onMount } from 'svelte';
  import { sessionsStore } from '$lib/stores/sessions.svelte';

  onMount(() => {
    void sessionsStore.init();
    return () => {
      sessionsStore.destroy();
    };
  });

  function agentLabel(agentIds: string[]) {
    return agentIds.length > 0 ? agentIds.join(', ') : 'Unknown';
  }

  function formatTimestamp(value: string | null | undefined) {
    if (!value) return 'Unknown';
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? 'Unknown' : date.toLocaleString();
  }
</script>

<h1 class="page-title">Sessions</h1>

{#if sessionsStore.loading}
  <div class="skeleton-table" aria-hidden="true">&nbsp;</div>
{:else if sessionsStore.error}
  <div class="error-state">
    <p>{sessionsStore.error}</p>
    <button onclick={() => sessionsStore.refresh()}>Retry</button>
  </div>
{:else if sessionsStore.list.length === 0}
  <div class="empty-state">
    <p>No sessions yet. Sessions appear when agents start working.</p>
  </div>
{:else}
  <div class="summary-bar">
    <span>{sessionsStore.list.length} of {sessionsStore.totalCount} sessions loaded</span>
  </div>

  <div class="table-shell">
    <table>
      <thead>
        <tr>
          <th>Session ID</th>
          <th>Agents</th>
          <th>Events</th>
          <th>Chain</th>
          <th>Started</th>
          <th>Last Event</th>
        </tr>
      </thead>
      <tbody>
        {#each sessionsStore.list as session}
          <tr>
            <td class="mono">
              <a href={`/sessions/${session.session_id}`} class="session-link" title={session.session_id}>
                {session.session_id.slice(0, 8)}…
              </a>
            </td>
            <td>{agentLabel(session.agent_ids ?? [])}</td>
            <td>{session.event_count}</td>
            <td>
              <span class:chain-valid={session.chain_valid} class:chain-broken={!session.chain_valid}>
                {session.chain_valid ? 'Valid' : 'Broken'}
              </span>
            </td>
            <td class="timestamp">{formatTimestamp(session.started_at)}</td>
            <td class="timestamp">{formatTimestamp(session.last_event_at)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#if sessionsStore.hasMore}
    <div class="load-more">
      <button onclick={() => sessionsStore.loadMore()} disabled={sessionsStore.loadingMore}>
        {sessionsStore.loadingMore ? 'Loading…' : 'Load More'}
      </button>
    </div>
  {/if}
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-4);
  }

  .summary-bar {
    margin-bottom: var(--spacing-3);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    min-width: 760px;
  }

  .table-shell {
    overflow-x: auto;
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
  }

  th {
    text-align: left;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
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
    font-size: var(--font-size-sm);
  }

  .timestamp {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .session-link {
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .session-link:hover {
    text-decoration: underline;
  }

  .chain-valid {
    color: var(--color-severity-normal);
  }

  .chain-broken {
    color: var(--color-severity-hard);
  }

  .skeleton-table {
    height: 200px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% {
      opacity: 0.4;
    }

    50% {
      opacity: 0.7;
    }
  }

  .empty-state,
  .error-state,
  .load-more {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
  }

  .error-state button,
  .load-more button {
    margin-top: var(--spacing-3);
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
  }
</style>
