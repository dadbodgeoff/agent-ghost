<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';

  onMount(() => {
    void goto('/channels', { replaceState: true, noScroll: true });
  });
</script>

<div class="redirect-page">
  <p>Channels moved to the canonical surface at <a href="/channels">/channels</a>.</p>
</div>

<style>
  .redirect-page {
    padding: var(--spacing-6);
  }
</style>
