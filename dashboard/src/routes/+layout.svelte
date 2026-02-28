<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';

  let token: string | null = null;
  let offline = $state(false);
  let showInstallPrompt = $state(false);
  let deferredPrompt: any = null;

  onMount(() => {
    token = sessionStorage.getItem('ghost-token');
    const currentPath = $page.url.pathname;
    if (!token && currentPath !== '/login') {
      goto('/login');
    }

    // Offline detection.
    offline = !navigator.onLine;
    window.addEventListener('online', () => (offline = false));
    window.addEventListener('offline', () => (offline = true));

    // PWA install prompt.
    window.addEventListener('beforeinstallprompt', (e: Event) => {
      e.preventDefault();
      deferredPrompt = e;
      showInstallPrompt = true;
    });

    // Register service worker.
    if ('serviceWorker' in navigator) {
      navigator.serviceWorker.register('/service-worker.js').catch(() => {
        // Service worker registration failed — non-fatal.
      });
    }

    // Subscribe to push notifications if permission granted.
    subscribeToPush();
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
      if (sub) return; // Already subscribed.

      // Fetch VAPID public key from gateway.
      const resp = await fetch('http://127.0.0.1:18789/api/push/vapid-key');
      if (!resp.ok) return;
      const { key } = await resp.json();

      const newSub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: key,
      });

      // Register subscription with gateway.
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
  .offline-banner { background: #ff6b35; color: #fff; text-align: center; padding: 6px; font-size: 13px; }
  .install-banner { background: #2a2a4e; color: #e0e0e0; text-align: center; padding: 8px; font-size: 13px; display: flex; align-items: center; justify-content: center; gap: 12px; }
  .install-banner button { background: #a0a0ff; color: #0d0d1a; border: none; padding: 4px 12px; border-radius: 4px; cursor: pointer; font-size: 12px; }
  .install-banner button:last-child { background: transparent; color: #888; }
</style>
