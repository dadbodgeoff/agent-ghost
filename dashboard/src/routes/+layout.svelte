<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import '../styles/global.css';
  import ConnectionIndicator from '../components/ConnectionIndicator.svelte';
  import CommandPalette from '../components/CommandPalette.svelte';
  import PanelLayout from '$lib/components/PanelLayout.svelte';
  import TabBar from '$lib/components/TabBar.svelte';
  import Terminal from '$lib/components/Terminal.svelte';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import { tabStore } from '$lib/stores/tabs.svelte';
  import { api, BASE_URL } from '$lib/api';

  const isTauri = typeof window !== 'undefined' && !!(window as any).__TAURI__;

  let token: string | null = null;
  let offline = $state(false);
  let showInstallPrompt = $state(false);
  let deferredPrompt: any = null;
  let lastSync = $state('unknown');

  // Reactive binding to WS connection state for the indicator.
  let wsState = $derived(wsStore.state);

  onMount(async () => {
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

    // Hydrate token from Tauri store before reading sessionStorage
    if (isTauri) {
      try {
        const { getToken } = await import('$lib/auth');
        const storedToken = await getToken();
        if (storedToken) sessionStorage.setItem('ghost-token', storedToken);
      } catch { /* auth module not yet available — non-fatal */ }
    }

    token = sessionStorage.getItem('ghost-token');
    const currentPath = $page.url.pathname;
    if (!token && currentPath !== '/login') {
      // Check if gateway requires auth before redirecting to login.
      // If health endpoint works without a token, auth is not configured — skip login.
      try {
        const healthResp = await fetch(`${BASE_URL}/api/agents`);
        if (!healthResp.ok) {
          goto('/login');
          return;
        }
        // No auth required — set a dummy token so stores/WS work.
        token = 'no-auth';
        sessionStorage.setItem('ghost-token', token);
      } catch {
        goto('/login');
        return;
      }
    }

    // Connect WebSocket store (replaces old api.connectWebSocket()).
    wsStore.connect();

    offline = !navigator.onLine;
    window.addEventListener('online', () => (offline = false));
    window.addEventListener('offline', () => {
      offline = true;
      lastSync = new Date().toLocaleTimeString();
    });

    window.addEventListener('beforeinstallprompt', (e: Event) => {
      e.preventDefault();
      deferredPrompt = e;
      showInstallPrompt = true;
    });

    // Service worker — browser only (Tauri doesn't need SW)
    if (!isTauri && 'serviceWorker' in navigator) {
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
    if (isTauri) {
      // Use Tauri notification plugin instead of web push
      try {
        const { isPermissionGranted, requestPermission } = await import('@tauri-apps/plugin-notification');
        const granted = await isPermissionGranted();
        if (!granted) await requestPermission();
      } catch { /* non-fatal */ }
      return;
    }
    if (!('PushManager' in window)) return;
    const permission = await Notification.requestPermission();
    if (permission !== 'granted') return;

    try {
      const reg = await navigator.serviceWorker.ready;
      const sub = await reg.pushManager.getSubscription();
      if (sub) return;

      const keyData = await api.get('/api/push/vapid-key');
      const key = keyData.key;
      if (!key) return;

      const newSub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: key,
      });

      await api.post('/api/push/subscribe', newSub.toJSON());
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
  <div class="offline-banner" role="alert">Offline — showing cached data (last sync: {lastSync})</div>
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
  <PanelLayout>
    {#snippet sidebar()}
      <nav aria-label="Main navigation">
        <div class="logo">GHOST</div>
        <a href="/" class:active={$page.url.pathname === '/'}>Overview</a>
        <a href="/convergence" class:active={$page.url.pathname === '/convergence'}>Convergence</a>
        <a href="/memory" class:active={$page.url.pathname === '/memory'}>Memory</a>
        <a href="/goals" class:active={$page.url.pathname === '/goals'}>Goals</a>
        <a href="/sessions" class:active={$page.url.pathname === '/sessions'}>Sessions</a>
        <a href="/agents" class:active={$page.url.pathname === '/agents'}>Agents</a>
        <a href="/workflows" class:active={$page.url.pathname.startsWith('/workflows')}>Workflows</a>
        <a href="/skills" class:active={$page.url.pathname.startsWith('/skills')}>Skills</a>
        <a href="/studio" class:active={$page.url.pathname.startsWith('/studio')}>Studio</a>
        <a href="/observability" class:active={$page.url.pathname.startsWith('/observability')}>Observability</a>
        <a href="/orchestration" class:active={$page.url.pathname.startsWith('/orchestration')}>Orchestration</a>
        <a href="/security" class:active={$page.url.pathname === '/security'}>Security</a>
        <a href="/costs" class:active={$page.url.pathname === '/costs'}>Costs</a>
        <a href="/search" class:active={$page.url.pathname === '/search'}>Search</a>
        <a href="/settings" class:active={$page.url.pathname.startsWith('/settings')}>Settings</a>
        {#if $page.url.pathname.startsWith('/settings')}
          <div class="settings-subnav">
            <a href="/settings/profiles" class:active={$page.url.pathname === '/settings/profiles'}>Profiles</a>
            <a href="/settings/policies" class:active={$page.url.pathname === '/settings/policies'}>Policies</a>
            <a href="/settings/channels" class:active={$page.url.pathname === '/settings/channels'}>Channels</a>
            <a href="/settings/backups" class:active={$page.url.pathname === '/settings/backups'}>Backups</a>
            <a href="/settings/webhooks" class:active={$page.url.pathname === '/settings/webhooks'}>Webhooks</a>
            <a href="/settings/notifications" class:active={$page.url.pathname === '/settings/notifications'}>Notifications</a>
            <a href="/settings/oauth" class:active={$page.url.pathname === '/settings/oauth'}>OAuth</a>
          </div>
        {/if}
      </nav>
    {/snippet}

    {#snippet sidebarFooter()}
      <ConnectionIndicator state={wsState} />
    {/snippet}

    {#snippet main()}
      <TabBar />
      <slot />
    {/snippet}

    {#snippet bottom()}
      <Terminal />
    {/snippet}
  </PanelLayout>
  <CommandPalette />
{/if}

<style>
  .logo {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-bold);
    color: var(--color-brand-primary);
    margin-bottom: var(--spacing-6);
    padding: var(--spacing-2);
  }

  nav a {
    display: block;
    color: var(--color-text-muted);
    padding: var(--spacing-2);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    transition: background var(--duration-fast) var(--easing-default),
                color var(--duration-fast) var(--easing-default);
  }

  nav a:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }

  nav a.active {
    background: var(--color-surface-selected);
    color: var(--color-brand-primary);
  }

  .settings-subnav {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
    padding-left: var(--spacing-4);
  }

  .settings-subnav a {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: var(--radius-sm);
  }

  .settings-subnav a:hover {
    color: var(--color-text-primary);
    background: var(--color-surface-hover);
  }

  .settings-subnav a.active {
    color: var(--color-brand-primary);
    background: var(--color-surface-selected);
  }

  .offline-banner {
    background: var(--color-severity-active);
    color: var(--color-text-inverse);
    text-align: center;
    padding: var(--spacing-1) var(--spacing-4);
    font-size: var(--font-size-sm);
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 200;
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
</style>
