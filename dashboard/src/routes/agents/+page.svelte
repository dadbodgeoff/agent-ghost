<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { agents } from '$lib/stores/agents';

  onMount(async () => {
    try {
      const data = await api.get('/api/agents') || [];
      agents.set(data);
    } catch (e) {
      console.error('Failed to load agents:', e);
    }
  });
</script>

<h1>Agents</h1>

{#if $agents.length === 0}
  <p class="empty">No agents registered</p>
{:else}
  {#each $agents as agent}
    <div class="agent-card">
      <div class="agent-name">{agent.name}</div>
      <div class="agent-meta">
        <span>Status: {agent.status}</span>
        <span>Score: {agent.convergenceScore?.toFixed(2) || '0.00'}</span>
        <span>Level: {agent.interventionLevel || 0}</span>
      </div>
    </div>
  {/each}
{/if}

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  .empty { color: #666; }
  .agent-card { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 16px; margin-bottom: 8px; }
  .agent-name { font-size: 16px; font-weight: 600; margin-bottom: 8px; }
  .agent-meta { display: flex; gap: 16px; font-size: 12px; color: #888; }
</style>
