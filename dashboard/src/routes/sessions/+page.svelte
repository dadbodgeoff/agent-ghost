<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import type {
    ListRuntimeSessionsCursorResult,
    ListRuntimeSessionsPageResult,
    RuntimeSession,
  } from '@ghost/sdk';

  let sessions: RuntimeSession[] = $state([]);
  let loading = $state(true);
  let error = $state('');

  onMount(async () => {
    try {
      const client = await getGhostClient();
      const data = await client.runtimeSessions.list();
      sessions = getSessions(data);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load sessions';
    }
    loading = false;
  });

  function getSessions(
    data: ListRuntimeSessionsPageResult | ListRuntimeSessionsCursorResult,
  ): RuntimeSession[] {
    return 'sessions' in data ? data.sessions : data.data;
  }
</script>

<h1 class="page-title">Sessions</h1>

{#if loading}
  <div class="skeleton-table">&nbsp;</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={() => location.reload()}>Retry</button>
  </div>
{:else if sessions.length === 0}
  <div class="empty-state">
    <p>No sessions yet. Sessions appear when agents start working.</p>
  </div>
{:else}
  <table>
    <thead>
      <tr>
        <th>Session ID</th>
        <th>Agents</th>
        <th>Events</th>
        <th>Started</th>
        <th>Last Event</th>
      </tr>
    </thead>
    <tbody>
      {#each sessions as session}
        <tr>
          <td class="mono"><a href="/sessions/{session.session_id}" class="session-link">{session.session_id.slice(0, 8)}…</a></td>
          <td>{Array.isArray(session.agents) ? session.agents.join(', ') : session.agents}</td>
          <td>{session.event_count}</td>
          <td class="timestamp">{new Date(session.started_at).toLocaleString()}</td>
          <td class="timestamp">{new Date(session.last_event_at).toLocaleString()}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  table {
    width: 100%;
    border-collapse: collapse;
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

  .skeleton-table {
    height: 200px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }

  .empty-state, .error-state {
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
</style>
