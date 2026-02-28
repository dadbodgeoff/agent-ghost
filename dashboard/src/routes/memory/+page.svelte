<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let memories: any[] = [];

  onMount(async () => {
    try {
      memories = await api.get('/api/memory') || [];
    } catch (e) {
      console.error('Failed to load memories:', e);
    }
  });
</script>

<h1>Memory</h1>

{#if memories.length === 0}
  <p class="empty">No memories loaded</p>
{:else}
  {#each memories as mem}
    <div class="memory-card">
      <div class="meta">
        <span class="type">{mem.memory_type || 'unknown'}</span>
        <span class="importance">{mem.importance || 'normal'}</span>
      </div>
      <p>{mem.content || ''}</p>
    </div>
  {/each}
{/if}

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  .empty { color: #666; }
  .memory-card { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 16px; margin-bottom: 8px; }
  .meta { display: flex; gap: 8px; margin-bottom: 8px; }
  .type, .importance { font-size: 11px; padding: 2px 6px; border-radius: 3px; background: #2a2a3e; }
  p { font-size: 13px; color: #ccc; }
</style>
