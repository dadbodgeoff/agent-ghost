<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';

  onMount(() => {
    goto('/channels', { replaceState: true });
  });
</script>

<svelte:head>
  <meta http-equiv="refresh" content="0;url=/channels" />
</svelte:head>

<div class="redirect-page">
  <p>Channels moved to the canonical surface at <a href="/channels">/channels</a>.</p>
</div>

<style>
  .redirect-page {
    padding: var(--spacing-6);
  }
</style>
