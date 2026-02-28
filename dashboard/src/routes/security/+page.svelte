<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let killState: any = null;
  let auditEntries: any[] = [];

  onMount(async () => {
    try {
      killState = await api.get('/api/safety/status');
      auditEntries = await api.get('/api/audit?page_size=20') || [];
    } catch (e) {
      console.error('Failed to load security data:', e);
    }
  });

  async function killAll() {
    if (confirm('Are you sure you want to trigger KILL_ALL? This will stop all agents.')) {
      await api.post('/api/safety/kill-all');
      killState = await api.get('/api/safety/status');
    }
  }
</script>

<h1>Security</h1>

<div class="kill-state">
  <span>Kill Switch: {killState?.level || 'Normal'}</span>
  <button class="danger" on:click={killAll}>KILL ALL</button>
</div>

<h2>Recent Audit Entries</h2>
{#if auditEntries.length === 0}
  <p class="empty">No audit entries</p>
{:else}
  {#each auditEntries as entry}
    <div class="audit-entry">
      <span class="time">{entry.timestamp}</span>
      <span class="type">{entry.event_type}</span>
      <span class="details">{entry.details}</span>
    </div>
  {/each}
{/if}

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  h2 { font-size: 14px; color: #888; margin: 24px 0 12px; }
  .empty { color: #666; }
  .kill-state { display: flex; justify-content: space-between; align-items: center; background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 16px; margin-bottom: 24px; }
  .danger { background: #4a0a0a; color: #f44336; border: 1px solid #f44336; padding: 8px 16px; border-radius: 4px; cursor: pointer; }
  .audit-entry { display: flex; gap: 12px; padding: 8px 0; border-bottom: 1px solid #1a1a2e; font-size: 12px; }
  .time { color: #666; width: 180px; }
  .type { color: #a0a0ff; width: 120px; }
  .details { color: #ccc; flex: 1; }
</style>
