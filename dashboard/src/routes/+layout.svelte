<script lang="ts">
  import type { GhostCompatibilityAssessment } from '@ghost/sdk';
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
  import { authSessionStore } from '$lib/stores/auth-session.svelte';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import { tabStore } from '$lib/stores/tabs.svelte';
  import { shortcuts } from '$lib/shortcuts';
  import {
    invalidateAuthClientState,
    isAuthResetError,
    notifyAuthBoundary,
    rotateAuthBoundarySession,
  } from '$lib/auth-boundary';
  import { getGhostClient } from '$lib/ghost-client';
  import { getRuntime, type RuntimePlatform } from '$lib/platform/runtime';

  let { children } = $props();
  let runtime: RuntimePlatform | null = null;
  let offline = $state(false);
  let bootError = $state('');
  let showInstallPrompt = $state(false);
  let deferredPrompt: any = null;
  let lastSync = $state('unknown');
  let unsubscribeTokenChange: (() => void) | null = null;
  let removeOnlineListener: (() => void) | null = null;
  let removeOfflineListener: (() => void) | null = null;
  let removeInstallPromptListener: (() => void) | null = null;

  let wsState = $derived(wsStore.state);

  async function handleKillAllShortcut() {
    if (confirm('Kill all agents? This cannot be undone.')) {
      const client = await getGhostClient();
      await client.safety.killAll('Kill switch via keyboard shortcut', 'dashboard_shortcut');
    }
  }

  function pushSubscriptionToPayload(subscription: PushSubscriptionJSON) {
    if (!subscription.endpoint) return null;
    return {
      endpoint: subscription.endpoint,
      keys: subscription.keys,
    };
  }

  function applyTheme() {
    const stored = localStorage.getItem('ghost-theme');
    document.documentElement.classList.remove('light');
    if (stored === 'light') {
      document.documentElement.classList.add('light');
    } else if (stored === 'system') {
      if (window.matchMedia('(prefers-color-scheme: light)').matches) {
        document.documentElement.classList.add('light');
      }
    }
  }

  function compatibilityMessage(assessment: GhostCompatibilityAssessment): string {
    const clientLabel = assessment.client.name === 'desktop' ? 'desktop' : 'dashboard';

    if (assessment.reason === 'unsupported_client') {
      return `This ${clientLabel} build is not recognized by gateway ${assessment.gatewayVersion}. Update the client before continuing.`;
    }

    if (assessment.reason === 'invalid_version') {
      return `This ${clientLabel} build reported an invalid version (${assessment.client.version}). Reinstall or update the client before continuing.`;
    }

    if (assessment.supportedRange) {
      return [
        `This ${clientLabel} build (${assessment.client.version}) is outside the gateway's supported range.`,
        `Gateway ${assessment.gatewayVersion} requires ${assessment.supportedRange.clientName} clients`,
        `from ${assessment.supportedRange.minimumVersion} up to`,
        `${assessment.supportedRange.maximumVersionExclusive} (exclusive).`,
      ].join(' ');
    }

    return `This ${clientLabel} build is incompatible with gateway ${assessment.gatewayVersion}. Update the client before continuing.`;
  }

  onMount(async () => {
    applyTheme();

    runtime = await getRuntime();
    unsubscribeTokenChange = runtime.subscribeTokenChange((token) => {
      if (!token) {
        authSessionStore.clear();
        return;
      }
      void authSessionStore.refresh().catch(() => {});
    });
    const currentPath = $page.url.pathname;

    try {
      const client = await getGhostClient();
      const compatibility = await client.compatibility.assessCurrentClient();
      if (!compatibility.supported) {
        bootError = compatibilityMessage(compatibility);
        return;
      }
    } catch {
      // Compatibility probe is advisory at startup; hard enforcement lives in the gateway.
    }

    if (currentPath !== '/login') {
      try {
        const client = await getGhostClient();
        const session = await client.auth.session();
        authSessionStore.hydrate(session);
        await notifyAuthBoundary('ghost-auth-session');
      } catch (error) {
        if (isAuthResetError(error)) {
          authSessionStore.clear();
          await rotateAuthBoundarySession();
          await runtime.clearToken();
          invalidateAuthClientState();
          await notifyAuthBoundary('ghost-auth-cleared');
          goto('/login');
          return;
        }
        bootError = 'Dashboard could not verify the current session. The gateway may be unavailable.';
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
    shortcuts.registerCommand('search.global', () => goto('/search'));
    shortcuts.registerCommand('studio.newSession', () => goto('/studio'));

    offline = !navigator.onLine;
    const handleOnline = () => {
      offline = false;
      lastSync = new Date().toLocaleTimeString();
    };
    const handleOffline = () => {
      offline = true;
      lastSync = new Date().toLocaleTimeString();
    };
    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);
    removeOnlineListener = () => window.removeEventListener('online', handleOnline);
    removeOfflineListener = () => window.removeEventListener('offline', handleOffline);

    const handleBeforeInstallPrompt = (e: Event) => {
      e.preventDefault();
      deferredPrompt = e;
      showInstallPrompt = true;
    };
    window.addEventListener('beforeinstallprompt', handleBeforeInstallPrompt);
    removeInstallPromptListener = () =>
      window.removeEventListener('beforeinstallprompt', handleBeforeInstallPrompt);

    if (!runtime.isDesktop() && 'serviceWorker' in navigator) {
      navigator.serviceWorker.register('/service-worker.js').catch(() => {});
    }

    await subscribeToPush();
  });

  onDestroy(() => {
    unsubscribeTokenChange?.();
    unsubscribeTokenChange = null;
    removeOnlineListener?.();
    removeOnlineListener = null;
    removeOfflineListener?.();
    removeOfflineListener = null;
    removeInstallPromptListener?.();
    removeInstallPromptListener = null;
    wsStore.disconnect();
    shortcuts.destroy();
  });

  $effect(() => {
    if (authSessionStore.canTriggerKillAll) {
      shortcuts.registerCommand('killSwitch.activateAll', handleKillAllShortcut);
      return;
    }
    shortcuts.unregisterCommand('killSwitch.activateAll');
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
        await runtime.requestNotificationPermission();
      } catch { /* non-fatal */ }
      return;
    }
    if (
      typeof window === 'undefined' ||
      typeof navigator === 'undefined' ||
      !('PushManager' in window) ||
      typeof Notification === 'undefined' ||
      !('serviceWorker' in navigator)
    ) {
      return;
    }
    if (Notification.permission !== 'granted') return;

    try {
      const client = await getGhostClient();
      const reg = await navigator.serviceWorker.ready;
      const sub = await reg.pushManager.getSubscription();
      if (sub) return;

      const keyData = await client.push.getVapidKey();
      const key = keyData.key;
      if (!key) return;

      const padding = '='.repeat((4 - (key.length % 4)) % 4);
      const normalized = (key + padding).replace(/-/g, '+').replace(/_/g, '/');
      const raw = atob(normalized);
      const applicationServerKey = Uint8Array.from(raw, (char) => char.charCodeAt(0));

      const newSub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey,
      });

      const payload = pushSubscriptionToPayload(newSub.toJSON());
      if (!payload) return;
      await client.push.subscribe(payload);
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

{#if bootError}
  <div class="offline-banner" role="alert">{bootError}</div>
{/if}

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
  {@render children?.()}
{:else}
  <PanelLayout>
    {#snippet sidebar()}
      <nav aria-label="Primary navigation">
        <div class="logo" role="banner">GHOST</div>
        <a href="/" class:active={$page.url.pathname === '/'} aria-current={$page.url.pathname === '/' ? 'page' : undefined}>Overview</a>
        <a href="/convergence" class:active={$page.url.pathname === '/convergence'} aria-current={$page.url.pathname === '/convergence' ? 'page' : undefined}>Convergence</a>
        <a href="/memory" class:active={$page.url.pathname.startsWith('/memory')} aria-current={$page.url.pathname.startsWith('/memory') ? 'page' : undefined}>Memory</a>
        <a href="/goals" class:active={$page.url.pathname.startsWith('/goals')} aria-current={$page.url.pathname.startsWith('/goals') ? 'page' : undefined}>Proposals</a>
        <a href="/sessions" class:active={$page.url.pathname.startsWith('/sessions')} aria-current={$page.url.pathname.startsWith('/sessions') ? 'page' : undefined}>Sessions</a>
        <a href="/agents" class:active={$page.url.pathname === '/agents'} aria-current={$page.url.pathname === '/agents' ? 'page' : undefined}>Agents</a>
        <a href="/workflows" class:active={$page.url.pathname.startsWith('/workflows')} aria-current={$page.url.pathname.startsWith('/workflows') ? 'page' : undefined}>Workflows</a>
        <a href="/skills" class:active={$page.url.pathname.startsWith('/skills')} aria-current={$page.url.pathname.startsWith('/skills') ? 'page' : undefined}>Skills</a>
        <a href="/studio" class:active={$page.url.pathname.startsWith('/studio')} aria-current={$page.url.pathname.startsWith('/studio') ? 'page' : undefined}>Studio</a>
        <a href="/channels" class:active={$page.url.pathname === '/channels'} aria-current={$page.url.pathname === '/channels' ? 'page' : undefined}>Channels</a>
        <a href="/observability" class:active={$page.url.pathname.startsWith('/observability')} aria-current={$page.url.pathname.startsWith('/observability') ? 'page' : undefined}>Observability</a>
        <a href="/orchestration" class:active={$page.url.pathname.startsWith('/orchestration')} aria-current={$page.url.pathname.startsWith('/orchestration') ? 'page' : undefined}>Orchestration</a>
        <a href="/pc-control" class:active={$page.url.pathname === '/pc-control'} aria-current={$page.url.pathname === '/pc-control' ? 'page' : undefined}>PC Control</a>
        <a href="/itp" class:active={$page.url.pathname === '/itp'} aria-current={$page.url.pathname === '/itp' ? 'page' : undefined}>ITP Events</a>
        <a href="/security" class:active={$page.url.pathname === '/security'} aria-current={$page.url.pathname === '/security' ? 'page' : undefined}>Security</a>
        <a href="/costs" class:active={$page.url.pathname === '/costs'} aria-current={$page.url.pathname === '/costs' ? 'page' : undefined}>Costs</a>
        <a href="/search" class:active={$page.url.pathname === '/search'} aria-current={$page.url.pathname === '/search' ? 'page' : undefined}>Search</a>
        <a href="/settings" class:active={$page.url.pathname.startsWith('/settings')} aria-current={$page.url.pathname.startsWith('/settings') ? 'page' : undefined}>Settings</a>
        {#if $page.url.pathname.startsWith('/settings')}
          <div class="settings-subnav">
            <a href="/settings/profiles" class:active={$page.url.pathname === '/settings/profiles'}>Profiles</a>
            <a href="/settings/policies" class:active={$page.url.pathname === '/settings/policies'}>Policies</a>
            <a href="/settings/providers" class:active={$page.url.pathname === '/settings/providers'}>Providers</a>
            <a href="/channels" class:active={$page.url.pathname === '/channels' || $page.url.pathname === '/settings/channels'}>Channels</a>
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
      {@render children?.()}
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
