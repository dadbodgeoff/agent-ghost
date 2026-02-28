<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { sessions } from '$lib/stores/sessions';

  onMount(async () => {
    try {
      const data = await api.get('/api/sessions') || [];
      sessions.set(data);
    } catch (e) {
      console.error('Failed to load sessions:', e);
    }
  });
</script>

<h1>Sessions</h1>

{#if $sessions.length === 0}
  <p class="empty">No active sessions</p>
{:else}
  <table>
    <thead>
      <tr><th>ID</th><th>Agent</th><th>Channel</th><th>Messages</th><th>Status</th></tr>
    </thead>
    <tbody>
      {#each $sessions as session}
        <tr>
          <td class="mono">{session.id.slice(0, 8)}</td>
          <td>{session.agentId}</td>
          <td>{session.channel}</td>
          <td>{session.messageCount}</td>
          <td>{session.status}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  .empty { color: #666; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; font-size: 11px; color: #888; text-transform: uppercase; padding: 8px; border-bottom: 1px solid #2a2a3e; }
  td { padding: 8px; font-size: 13px; border-bottom: 1px solid #1a1a2e; }
  .mono { font-family: monospace; font-size: 12px; }
</style>
