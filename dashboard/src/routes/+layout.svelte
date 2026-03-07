<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import '../styles/global.css';
  import ConnectionIndicator from '../components/ConnectionIndicator.svelte';
  import CommandPalette from '../components/CommandPalette.svelte';
  import Breadcrumb from '../components/Breadcrumb.svelte';
  import NotificationPanel from '../components/NotificationPanel.svelte';
  import PanelLayout from '$lib/components/PanelLayout.svelte';
  import TabBar from '$lib/components/TabBar.svelte';
  import Terminal from '$lib/components/Terminal.svelte';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import { tabStore } from '$lib/stores/tabs.svelte';
  import { shortcuts } from '$lib/shortcuts';
  import { api } from '$lib/api';
  import { clearToken } from '$lib/auth';
  import { getGhostClient } from '$lib/ghost-client';
  import { getRuntime, type RuntimePlatform } from '$lib/platform/runtime';
  import { GhostAPIError } from '@ghost/sdk';

  let runtime: RuntimePlatform | null = null;
  let offline = $state(false);
  let showInstallPrompt = $state(false);
  let deferredPrompt: any = null;
  let lastSync = $state('unknown');

  let wsState = $derived(wsStore.state);

  function applyTheme() {
    const stored = localStorage.getItem('ghost-theme');
    if (stored === 'light') {
      document.documentElement.classList.add('light');
    } else if (stored === 'system') {
      if (window.matchMedia('(prefers-color-scheme: light)').matches) {
        document.documentElement.classList.add('light');
      }
    }
  }

  onMount(async () => {
    applyTheme();

    runtime = await getRuntime();
    const currentPath = $page.url.pathname;

    if (currentPath !== '/login') {
      try {
        const client = await getGhostClient();
        await client.agents.list();
      } catch (error) {
        if (error instanceof GhostAPIError && error.status === 401) {
          await clearToken();
        }
        goto('/login');
        return;
      }
    }

    await wsStore.connect();

    shortcuts.init();
    shortcuts.registerCommand('sidebar.toggle', () => { /* PanelLayout handles */ });
    shortcuts.registerCommand('theme.toggle', () => {
      document.documentElement.classList.toggle('light');
      const isLight = document.documentElement.classList.contains('light');
      localStorage.setItem('ghost-theme', isLight ? 'light' : 'dark');
    });
    shortcuts.registerCommand('killSwitch.activateAll', async () => {
      if (confirm('Kill all agents? This cannot be undone.')) {
        const client = await getGhostClient();
        await client.safety.killAll('Kill switch via keyboard shortcut', 'dashboard_shortcut');
      }
    });
    shortcuts.registerCommand('search.global', () => goto('/search'));
    shortcuts.registerCommand('studio.newSession', () => goto('/studio'));

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

    if (!runtime.isDesktop() && 'serviceWorker' in navigator) {
      navigator.serviceWorker.register('/service-worker.js').catch(() => {});
    }

    await subscribeToPush();
  });

  onDestroy(() => {
    wsStore.disconnect();
    shortcuts.destroy();
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
    if (runtime?.isDesktop()) {
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

      const keyData = await api.get<{ key?: string }>('/api/push/vapid-key');
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
      <nav aria-label="Primary navigation" role="navigation">
        <div class="logo" role="banner">GHOST</div>
        <a href="/" class:active={$page.url.pathname === '/'} aria-current={$page.url.pathname === '/' ? 'page' : undefined}>Overview</a>
        <a href="/convergence" class:active={$page.url.pathname === '/convergence'} aria-current={$page.url.pathname === '/convergence' ? 'page' : undefined}>Convergence</a>
        <a href="/memory" class:active={$page.url.pathname.startsWith('/memory')} aria-current={$page.url.pathname.startsWith('/memory') ? 'page' : undefined}>Memory</a>
        <a href="/goals" class:active={$page.url.pathname === '/goals'} aria-current={$page.url.pathname === '/goals' ? 'page' : undefined}>Goals</a>
        <a href="/sessions" class:active={$page.url.pathname === '/sessions'} aria-current={$page.url.pathname === '/sessions' ? 'page' : undefined}>Sessions</a>
        <a href="/agents" class:active={$page.url.pathname === '/agents'} aria-current={$page.url.pathname === '/agents' ? 'page' : undefined}>Agents</a>
        <a href="/workflows" class:active={$page.url.pathname.startsWith('/workflows')} aria-current={$page.url.pathname.startsWith('/workflows') ? 'page' : undefined}>Workflows</a>
        <a href="/skills" class:active={$page.url.pathname.startsWith('/skills')} aria-current={$page.url.pathname.startsWith('/skills') ? 'page' : undefined}>Skills</a>
        <a href="/studio" class:active={$page.url.pathname.startsWith('/studio')} aria-current={$page.url.pathname.startsWith('/studio') ? 'page' : undefined}>Studio</a>
        <a href="/channels" class:active={$page.url.pathname === '/channels'} aria-current={$page.url.pathname === '/channels' ? 'page' : undefined}>Channels</a>
        <a href="/observability" class:active={$page.url.pathname.startsWith('/observability')} aria-current={$page.url.pathname.startsWith('/observability') ? 'page' : undefined}>Observability</a>
        <a href="/orchestration" class:active={$page.url.pathname.startsWith('/orchestration')} aria-current={$page.url.pathname.startsWith('/orchestration') ? 'page' : undefined}>Orchestration</a>
        <a href="/pc-control" class:active={$page.url.pathname === '/pc-control'} aria-current={$page.url.pathname === '/pc-control' ? 'page' : undefined}>PC Control</a>
        <a href="/itp" class:active={$page.url.pathname === '/itp'} aria-current={$page.url.pathname === '/itp' ? 'page' : undefined}>ITP Events</a>
        <a href="/approvals" class:active={$page.url.pathname === '/approvals'} aria-current={$page.url.pathname === '/approvals' ? 'page' : undefined}>Approvals</a>
        <a href="/security" class:active={$page.url.pathname === '/security'} aria-current={$page.url.pathname === '/security' ? 'page' : undefined}>Security</a>
        <a href="/costs" class:active={$page.url.pathname === '/costs'} aria-current={$page.url.pathname === '/costs' ? 'page' : undefined}>Costs</a>
        <a href="/search" class:active={$page.url.pathname === '/search'} aria-current={$page.url.pathname === '/search' ? 'page' : undefined}>Search</a>
        <a href="/settings" class:active={$page.url.pathname.startsWith('/settings')} aria-current={$page.url.pathname.startsWith('/settings') ? 'page' : undefined}>Settings</a>
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
      <div class="sidebar-footer-row">
        <ConnectionIndicator state={wsState} />
        <NotificationPanel />
      </div>
    {/snippet}

    {#snippet main()}
      <TabBar />
      <Breadcrumb />
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

  .sidebar-footer-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-2);
  }
</style>
