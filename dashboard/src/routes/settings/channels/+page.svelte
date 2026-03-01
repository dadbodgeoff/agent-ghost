<script lang="ts">
  /**
   * Channel configuration — list, add, remove channels.
   *
   * Ref: T-3.11.2
   */
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  interface Channel {
    name: string;
    type: string;
    status: string;
    agent_id?: string;
  }

  let channels: Channel[] = $state([]);
  let error: string | null = $state(null);

  onMount(() => {
    loadChannels();
  });

  async function loadChannels() {
    try {
      const res = await api.get('/api/agents');
      // T-5.9.4: Standardize — API returns array directly for /api/agents.
      const agents: Array<{ id: string; name: string; status?: string }> = Array.isArray(res) ? res : (res?.agents ?? []);
      channels = agents.map((a: { id: string; name: string; status?: string }) => ({
        name: a.name,
        type: 'agent',
        status: a.status ?? 'active',
        agent_id: a.id,
      }));
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load channels';
    }
  }
</script>

<svelte:head>
  <title>Channels | Settings | ADE</title>
</svelte:head>

<div class="channels-page">
  <header class="page-header">
    <h1>Channels</h1>
    <p class="subtitle">Configure agent communication channels and bindings</p>
  </header>

  {#if error}
    <p class="error-msg">{error}</p>
  {/if}

  {#if channels.length === 0}
    <p class="empty">No channels configured. Channels are created when agents are registered.</p>
  {:else}
    <table class="data-table">
      <thead>
        <tr>
          <th>Name</th>
          <th>Type</th>
          <th>Status</th>
          <th>Agent</th>
        </tr>
      </thead>
      <tbody>
        {#each channels as ch}
          <tr>
            <td>{ch.name}</td>
            <td>{ch.type}</td>
            <td>
              <span class="status-dot" class:active={ch.status === 'active'}></span>
              {ch.status}
            </td>
            <td class="mono">{ch.agent_id ? ch.agent_id.slice(0, 8) + '…' : '—'}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .channels-page { padding: var(--spacing-6); max-width: 900px; }
  .page-header { margin-bottom: var(--spacing-6); }
  .page-header h1 { font-size: var(--font-size-2xl); font-weight: 700; color: var(--color-text-primary); }
  .subtitle { color: var(--color-text-muted); font-size: var(--font-size-sm); margin-top: var(--spacing-1); }

  .data-table { width: 100%; border-collapse: collapse; font-size: var(--font-size-sm); }
  .data-table th {
    text-align: left; padding: var(--spacing-2) var(--spacing-3); background: var(--color-bg-elevated-1);
    color: var(--color-text-muted); font-weight: 600; font-size: var(--font-size-xs);
    text-transform: uppercase; border-bottom: 1px solid var(--color-border-default);
  }
  .data-table td { padding: var(--spacing-2) var(--spacing-3); border-bottom: 1px solid var(--color-border-subtle); color: var(--color-text-primary); }

  .status-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--color-text-muted); margin-right: var(--spacing-1); vertical-align: middle; }
  .status-dot.active { background: var(--color-severity-normal); }

  .empty { text-align: center; padding: var(--spacing-8); color: var(--color-text-muted); font-size: var(--font-size-sm); }
  .error-msg { color: var(--color-severity-hard); font-size: var(--font-size-sm); padding: var(--spacing-3); background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-hard); border-radius: var(--radius-sm); margin-bottom: var(--spacing-3); }
  .mono { font-family: var(--font-family-mono); font-variant-numeric: tabular-nums; }
</style>
