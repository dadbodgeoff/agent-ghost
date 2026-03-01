<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import '../styles/global.css';
  import ConnectionIndicator from '../components/ConnectionIndicator.svelte';
  import { wsStore } from '$lib/stores/websocket.svelte';

  let token: string | null = null;
  let offline = $state(false);
  let showInstallPrompt = $state(false);
  let deferredPrompt: any = null;

  // Reactive binding to WS connection state for the indicator.
  let wsState = $derived(wsStore.state);

  onMount(() => {
    // Theme detection: localStorage → prefers-color-scheme → dark default.
    const stored = localStorage.getItem('ghost-theme');
    if (stored === 'light') {
      document.documentElement.classList.add('light');
    } else if (stored === 'system') {
      if (window.matchMedia('(prefers-color-scheme: light)').matches) {
        document.documentElement.classList.add('light');
      }
    }
    // Default (null or 'dark') = no .light class = dark theme.

    token = sessionStorage.getItem('ghost-token');
    const currentPath = $page.url.pathname;
    if (!token && currentPath !== '/login') {
      goto('/login');
      return;
    }

    // Connect WebSocket store (replaces old api.connectWebSocket()).
    if (token) {
      wsStore.connect();
    }

    offline = !navigator.onLine;
    window.addEventListener('online', () => (offline = false));
    window.addEventListener('offline', () => (offline = true));

    window.addEventListener('beforeinstallprompt', (e: Event) => {
      e.preventDefault();
      deferredPrompt = e;
      showInstallPrompt = true;
    });

    if ('serviceWorker' in navigator) {
      navigator.serviceWorker.register('/service-worker.js').catch(() => {});
    }

    subscribeToPush();
  });

  onDestroy(() => {
    wsStore.disconnect();
  });

  async function installPWA() {
    if (!deferredPrompt) return;
    deferredPrompt.prompt();
    const result = await deferredPrompt.userChoice;
    if (result.outcome === 'accepted') {
      showInstallPrompt = false;
    }
    deferredPrompt = null;
  }

  async function subscribeToPush() {
    if (!('PushManager' in window)) return;
    const permission = await Notification.requestPermission();
    if (permission !== 'granted') return;

    try {
      const reg = await navigator.serviceWorker.ready;
      const sub = await reg.pushManager.getSubscription();
      if (sub) return;

      const resp = await fetch('http://127.0.0.1:18789/api/push/vapid-key');
      if (!resp.ok) return;
      const { key } = await resp.json();

      const newSub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: key,
      });

      const authToken = sessionStorage.getItem('ghost-token');
      await fetch('http://127.0.0.1:18789/api/push/subscribe', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
        },
        body: JSON.stringify(newSub.toJSON()),
      });
    } catch {
      // Push subscription failed — non-fatal.
    }
  }
</script>

<svelte:head>
  <link rel="manifest" href="/manifest.json" />
  <meta name="theme-color" content="#1a1a2e" />
  <meta name="apple-mobile-web-app-capable" content="yes" />
  <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
</svelte:head>

{#if offline}
  <div class="offline-banner" role="alert">Offline — showing cached data</div>
{/if}

{#if showInstallPrompt}
  <div class="install-banner">
    <span>Install GHOST Dashboard for quick access</span>
    <button onclick={installPWA}>Install</button>
    <button onclick={() => (showInstallPrompt = false)}>Dismiss</button>
  </div>
{/if}

{#if $page.url.pathname === '/login'}
  <slot />
{:else}
  <div class="layout">
    <nav class="sidebar" aria-label="Main navigation">
      <div class="logo">GHOST</div>
      <a href="/" class:active={$page.url.pathname === '/'}>Overview</a>
      <a href="/convergence" class:active={$page.url.pathname === '/convergence'}>Convergence</a>
      <a href="/memory" class:active={$page.url.pathname === '/memory'}>Memory</a>
      <a href="/goals" class:active={$page.url.pathname === '/goals'}>Goals</a>
      <a href="/sessions" class:active={$page.url.pathname === '/sessions'}>Sessions</a>
      <a href="/agents" class:active={$page.url.pathname === '/agents'}>Agents</a>
      <a href="/security" class:active={$page.url.pathname === '/security'}>Security</a>
      <a href="/costs" class:active={$page.url.pathname === '/costs'}>Costs</a>
      <a href="/settings" class:active={$page.url.pathname.startsWith('/settings')}>Settings</a>

      <div class="sidebar-footer">
        <ConnectionIndicator state={wsState} />
      </div>
    </nav>
    <main class="content">
      <slot />
    </main>
  </div>
{/if}

<style>
  .layout {
    display: flex;
    min-height: 100vh;
  }

  .sidebar {
    width: var(--layout-sidebar-width);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-4);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
    border-right: 1px solid var(--color-border-default);
    position: sticky;
    top: 0;
    height: 100vh;
    overflow-y: auto;
  }

  .logo {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-bold);
    color: var(--color-brand-primary);
    margin-bottom: var(--spacing-6);
    padding: var(--spacing-2);
  }

  .sidebar a {
    color: var(--color-text-muted);
    padding: var(--spacing-2);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    transition: background var(--duration-fast) var(--easing-default),
                color var(--duration-fast) var(--easing-default);
  }

  .sidebar a:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }

  .sidebar a.active {
    background: var(--color-surface-selected);
    color: var(--color-brand-primary);
  }

  .sidebar-footer {
    margin-top: auto;
    padding-top: var(--spacing-4);
    border-top: 1px solid var(--color-border-subtle);
  }

  .content {
    flex: 1;
    padding: var(--layout-content-padding);
    max-width: var(--layout-content-max-width);
  }

  .offline-banner {
    background: var(--color-severity-active);
    color: var(--color-text-inverse);
    text-align: center;
    padding: var(--spacing-1) var(--spacing-4);
    font-size: var(--font-size-sm);
  }

  .install-banner {
    background: var(--color-surface-selected);
    color: var(--color-text-primary);
    text-align: center;
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-3);
  }

  .install-banner button {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
  }

  .install-banner button:last-child {
    background: transparent;
    color: var(--color-text-muted);
  }

  @media (max-width: 640px) {
    .sidebar { display: none; }
    .content { padding: var(--spacing-4); }
  }

  @media (min-width: 641px) and (max-width: 1024px) {
    .sidebar { width: var(--layout-sidebar-collapsed); }
    .sidebar a { font-size: 0; padding: var(--spacing-3); text-align: center; }
    .sidebar a::first-letter { font-size: var(--font-size-sm); }
    .sidebar-footer { display: none; }
  }
</style>
