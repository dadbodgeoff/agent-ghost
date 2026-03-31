<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';

  const CATEGORIES = [
    { id: 'intervention', label: 'Intervention Changes', desc: 'Agent paused, resumed, quarantined' },
    { id: 'kill_switch', label: 'Kill Switch', desc: 'KILL_ALL activations' },
    { id: 'proposals', label: 'Proposals', desc: 'Proposal approvals and rejections' },
    { id: 'lifecycle', label: 'Agent Lifecycle', desc: 'Agent state transitions' },
    { id: 'backup', label: 'Backup Complete', desc: 'Automated backup completion' },
  ];

  let pushEnabled = $state(false);
  let pushSupported = $state(false);
  let permissionState = $state<NotificationPermission>('default');
  let enabledCategories = $state<string[]>(['intervention', 'kill_switch']);
  let testSending = $state(false);

  function decodeApplicationServerKey(key: string): ArrayBuffer {
    const padding = '='.repeat((4 - (key.length % 4)) % 4);
    const normalized = (key + padding).replace(/-/g, '+').replace(/_/g, '/');
    const raw = atob(normalized);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i += 1) {
      bytes[i] = raw.charCodeAt(i);
    }
    return bytes.buffer;
  }

  function pushSubscriptionToPayload(subscription: PushSubscriptionJSON) {
    if (!subscription.endpoint) return null;
    return {
      endpoint: subscription.endpoint,
      keys: subscription.keys,
    };
  }

  async function getReadyServiceWorkerRegistration(): Promise<ServiceWorkerRegistration | null> {
    if (!('serviceWorker' in navigator)) {
      return null;
    }
    return navigator.serviceWorker.ready.catch(() => null);
  }

  onMount(async () => {
    pushSupported =
      'serviceWorker' in navigator &&
      'PushManager' in window &&
      'Notification' in window;
    if (!pushSupported) {
      return;
    }

    permissionState = Notification.permission;

    const saved = localStorage.getItem('ghost-push-categories');
    if (saved) {
      try {
        enabledCategories = JSON.parse(saved);
      } catch {
        // Use defaults if preferences are malformed.
      }
    }

    if (permissionState !== 'granted') {
      pushEnabled = false;
      return;
    }

    const reg = await getReadyServiceWorkerRegistration();
    const existingSub = await reg?.pushManager.getSubscription().catch(() => null);
    pushEnabled = !!existingSub;
  });

  async function togglePush() {
    if (!pushSupported) return;

    if (!pushEnabled) {
      const permission = await Notification.requestPermission();
      permissionState = permission;
      if (permission === 'granted') {
        pushEnabled = await subscribePush();
      }
    } else {
      await unsubscribePush();
      pushEnabled = false;
    }
  }

  async function subscribePush(): Promise<boolean> {
    try {
      const reg = await getReadyServiceWorkerRegistration();
      if (!reg) return false;

      const existing = await reg.pushManager.getSubscription();
      if (existing) {
        return true;
      }

      const client = await getGhostClient();
      const keyData = await client.push.getVapidKey();
      if (!keyData.key) return false;

      const sub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: decodeApplicationServerKey(keyData.key),
      });
      const payload = pushSubscriptionToPayload(sub.toJSON());
      if (!payload) return false;
      await client.push.subscribe(payload);
      return true;
    } catch {
      return false;
    }
  }

  async function unsubscribePush() {
    try {
      const reg = await getReadyServiceWorkerRegistration();
      if (!reg) return;

      const client = await getGhostClient();
      const sub = await reg.pushManager.getSubscription();
      if (sub) {
        const payload = pushSubscriptionToPayload(sub.toJSON());
        if (payload) {
          await client.push.unsubscribe(payload);
        }
        await sub.unsubscribe();
      }
    } catch {
      // Unsubscribe failed.
    }
  }

  function toggleCategory(id: string) {
    if (enabledCategories.includes(id)) {
      enabledCategories = enabledCategories.filter((c) => c !== id);
    } else {
      enabledCategories = [...enabledCategories, id];
    }
    localStorage.setItem('ghost-push-categories', JSON.stringify(enabledCategories));
  }

  async function sendTestNotification() {
    testSending = true;
    try {
      const reg = await getReadyServiceWorkerRegistration();
      if (!reg) return;

      await reg.showNotification('GHOST Test', {
        body: 'Push notifications are working correctly.',
        icon: '/icons/ghost-icon.svg',
        badge: '/icons/ghost-icon.svg',
        tag: 'ghost-test',
      });
    } catch {
      // Test failed.
    } finally {
      testSending = false;
    }
  }
</script>

<div class="page">
  <header class="page-header">
    <h1>Notifications</h1>
    <p class="subtitle">Configure push notification preferences</p>
  </header>

  {#if !pushSupported}
    <div class="unsupported">
      Push notifications are not supported in this browser.
    </div>
  {:else}
    <div class="section">
      <div class="toggle-row">
        <div>
          <span class="toggle-label">Push Notifications</span>
          <span class="toggle-desc">
            {#if permissionState === 'denied'}
              Blocked by browser — check site permissions
            {:else if pushEnabled}
              Enabled — receiving notifications
            {:else}
              Disabled — click to enable
            {/if}
          </span>
        </div>
        <button
          class="toggle-btn"
          class:active={pushEnabled}
          disabled={permissionState === 'denied'}
          onclick={togglePush}
        >
          {pushEnabled ? 'ON' : 'OFF'}
        </button>
      </div>
    </div>

    {#if pushEnabled}
      <div class="section">
        <h2>Notification Categories</h2>
        <div class="category-list">
          {#each CATEGORIES as cat}
            <label class="category-row">
              <input
                type="checkbox"
                checked={enabledCategories.includes(cat.id)}
                onchange={() => toggleCategory(cat.id)}
              />
              <div>
                <span class="cat-label">{cat.label}</span>
                <span class="cat-desc">{cat.desc}</span>
              </div>
            </label>
          {/each}
        </div>
      </div>

      <div class="section">
        <button
          class="test-btn"
          disabled={testSending}
          onclick={sendTestNotification}
        >
          {testSending ? 'Sending...' : 'Send Test Notification'}
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-6);
    max-width: 640px;
  }

  .page-header h1 {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
    margin: 0;
  }

  .subtitle {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin: var(--spacing-1) 0 0;
  }

  .unsupported,
  .section {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
  }

  .unsupported {
    color: var(--color-text-muted);
  }

  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-4);
  }

  .toggle-label {
    display: block;
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .toggle-desc {
    display: block;
    margin-top: var(--spacing-1);
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .toggle-btn,
  .test-btn {
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    background: var(--color-bg-elevated-3);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    padding: var(--spacing-2) var(--spacing-4);
    transition:
      background var(--duration-fast) var(--easing-default),
      border-color var(--duration-fast) var(--easing-default),
      color var(--duration-fast) var(--easing-default);
  }

  .toggle-btn.active {
    background: var(--color-brand-subtle);
    border-color: var(--color-brand-primary);
    color: var(--color-brand-primary);
  }

  .toggle-btn:disabled,
  .test-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .category-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    margin-top: var(--spacing-4);
  }

  .category-row {
    display: flex;
    align-items: flex-start;
    gap: var(--spacing-3);
    color: var(--color-text-primary);
  }

  .cat-label {
    display: block;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
  }

  .cat-desc {
    display: block;
    margin-top: var(--spacing-1);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
</style>
