<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let reflections: any[] = [];

  onMount(async () => {
    try {
      reflections = await api.get('/api/reflections') || [];
    } catch (e) {
      console.error('Failed to load reflections:', e);
    }
  });
</script>

<h1>Reflections</h1>

{#if reflections.length === 0}
  <p class="empty">No reflections recorded</p>
{:else}
  {#each reflections as ref}
    <div class="reflection-card">
      <div class="meta">
        <span class="trigger">{ref.trigger || 'manual'}</span>
        <span class="depth">Depth {ref.depth || 0}</span>
      </div>
      <p>{ref.text || ''}</p>
    </div>
  {/each}
{/if}

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  .empty { color: #666; }
  .reflection-card { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 16px; margin-bottom: 8px; }
  .meta { display: flex; gap: 8px; margin-bottom: 8px; }
  .trigger, .depth { font-size: 11px; padding: 2px 6px; border-radius: 3px; background: #2a2a3e; }
  p { font-size: 13px; color: #ccc; }
</style>
