<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';

  let token: string | null = null;

  onMount(() => {
    token = sessionStorage.getItem('ghost-token');
    const currentPath = $page.url.pathname;
    if (!token && currentPath !== '/login') {
      goto('/login');
    }
  });
</script>

{#if $page.url.pathname === '/login'}
  <slot />
{:else}
  <div class="layout">
    <nav class="sidebar">
      <div class="logo">GHOST</div>
      <a href="/" class:active={$page.url.pathname === '/'}>Overview</a>
      <a href="/convergence" class:active={$page.url.pathname === '/convergence'}>Convergence</a>
      <a href="/memory" class:active={$page.url.pathname === '/memory'}>Memory</a>
      <a href="/goals" class:active={$page.url.pathname === '/goals'}>Goals</a>
      <a href="/reflections" class:active={$page.url.pathname === '/reflections'}>Reflections</a>
      <a href="/sessions" class:active={$page.url.pathname === '/sessions'}>Sessions</a>
      <a href="/agents" class:active={$page.url.pathname === '/agents'}>Agents</a>
      <a href="/security" class:active={$page.url.pathname === '/security'}>Security</a>
      <a href="/settings" class:active={$page.url.pathname === '/settings'}>Settings</a>
    </nav>
    <main class="content">
      <slot />
    </main>
  </div>
{/if}

<style>
  :global(body) { margin: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: #0d0d1a; color: #e0e0e0; }
  .layout { display: flex; min-height: 100vh; }
  .sidebar { width: 200px; background: #1a1a2e; padding: 16px; display: flex; flex-direction: column; gap: 4px; border-right: 1px solid #2a2a3e; }
  .logo { font-size: 18px; font-weight: 700; color: #a0a0ff; margin-bottom: 24px; padding: 8px; }
  .sidebar a { color: #888; text-decoration: none; padding: 8px; border-radius: 4px; font-size: 13px; }
  .sidebar a:hover { background: #2a2a3e; color: #e0e0e0; }
  .sidebar a.active { background: #2a2a4e; color: #a0a0ff; }
  .content { flex: 1; padding: 24px; max-width: 1200px; }
</style>
