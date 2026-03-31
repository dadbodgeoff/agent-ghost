<script lang="ts">
  import { onMount } from 'svelte';
  import { readLocalStorage, writeLocalStorage } from '$lib/browser';
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
  let statusMessage = $state('');
  let errorMessage = $state('');

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

  onMount(async () => {
    pushSupported = 'PushManager' in window && 'Notification' in window;
    if (pushSupported) {
      permissionState = Notification.permission;
      pushEnabled = permissionState === 'granted';

      // Load saved preferences.
      const saved = readLocalStorage('ghost-push-categories');
      if (saved) {
        try {
          const parsed = JSON.parse(saved);
          if (Array.isArray(parsed)) {
            enabledCategories = parsed.filter((value): value is string => typeof value === 'string');
          }
        } catch { /* use defaults */ }
      }
    }
  });

  async function togglePush() {
    if (!pushSupported) return;
    statusMessage = '';
    errorMessage = '';

    if (!pushEnabled) {
      const permission = await Notification.requestPermission();
      permissionState = permission;
      if (permission === 'granted') {
        const subscribed = await subscribePush();
        pushEnabled = subscribed;
        if (!subscribed) {
          errorMessage = 'Push permission was granted, but the subscription could not be completed.';
        }
      }
    } else {
      const unsubscribed = await unsubscribePush();
      if (unsubscribed) {
        pushEnabled = false;
      } else {
        errorMessage = 'Push notifications could not be disabled cleanly.';
      }
    }
  }

  async function subscribePush(): Promise<boolean> {
    try {
      if (!('serviceWorker' in navigator)) return false;
      const client = await getGhostClient();
      const reg = await navigator.serviceWorker.getRegistration();
      if (!reg) return false;
      const keyData = await client.push.getVapidKey();
      if (!keyData.key) return false;

      const existing = await reg.pushManager.getSubscription();
      const sub = existing ?? await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: decodeApplicationServerKey(keyData.key),
      });
      const payload = pushSubscriptionToPayload(sub.toJSON());
      if (!payload) return false;
      await client.push.subscribe(payload);
      statusMessage = 'Push notifications enabled.';
      return true;
    } catch {
      return false;
    }
  }

  async function unsubscribePush(): Promise<boolean> {
    try {
      if (!('serviceWorker' in navigator)) return false;
      const client = await getGhostClient();
      const reg = await navigator.serviceWorker.getRegistration();
      if (!reg) return false;
      const sub = await reg.pushManager.getSubscription();
      if (sub) {
        const payload = pushSubscriptionToPayload(sub.toJSON());
        if (payload) {
          await client.push.unsubscribe(payload);
        }
        await sub.unsubscribe();
      }
      statusMessage = 'Push notifications disabled.';
      return true;
    } catch {
      return false;
    }
  }

  function toggleCategory(id: string) {
    if (enabledCategories.includes(id)) {
      enabledCategories = enabledCategories.filter(c => c !== id);
    } else {
      enabledCategories = [...enabledCategories, id];
    }
    writeLocalStorage('ghost-push-categories', JSON.stringify(enabledCategories));
  }

  async function sendTestNotification() {
    testSending = true;
    statusMessage = '';
    errorMessage = '';
    try {
      if (!('serviceWorker' in navigator)) {
        throw new Error('Service worker unavailable');
      }
      const reg = await navigator.serviceWorker.getRegistration();
      if (!reg) {
        throw new Error('Service worker not registered');
      }
      await reg.showNotification('GHOST Test', {
        body: 'Push notifications are working correctly.',
        icon: '/icons/ghost-192.png',
        tag: 'ghost-test',
      });
      statusMessage = 'Test notification sent.';
    } catch {
      errorMessage = 'Test notification failed. Check browser notification and service worker permissions.';
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

  {#if errorMessage}
    <div class="status error" role="alert">{errorMessage}</div>
  {/if}

  {#if statusMessage}
    <div class="status success" role="status">{statusMessage}</div>
  {/if}

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

  .unsupported {
    background: color-mix(in srgb, var(--color-severity-active) 15%, transparent);
    color: var(--color-severity-active);
    padding: var(--spacing-3) var(--spacing-4);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
  }

  .status {
    padding: var(--spacing-3) var(--spacing-4);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
  }

  .status.error {
    background: var(--color-severity-hard-bg);
    color: var(--color-severity-hard);
    border: 1px solid var(--color-severity-hard);
  }

  .status.success {
    background: color-mix(in srgb, var(--color-severity-normal) 15%, transparent);
    color: var(--color-severity-normal);
    border: 1px solid color-mix(in srgb, var(--color-severity-normal) 40%, transparent);
  }

  .section {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .section h2 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    margin: 0 0 var(--spacing-3);
  }

  .toggle-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-4);
  }

  .toggle-label {
    display: block;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .toggle-desc {
    display: block;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-top: var(--spacing-1);
  }

  .toggle-btn {
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-bold);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-base);
    color: var(--color-text-muted);
    cursor: pointer;
    min-width: 48px;
  }

  .toggle-btn.active {
    background: var(--color-score-high);
    color: var(--color-text-inverse);
    border-color: var(--color-score-high);
  }

  .toggle-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .category-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .category-row {
    display: flex;
    align-items: flex-start;
    gap: var(--spacing-3);
    cursor: pointer;
  }

  .category-row input[type="checkbox"] {
    accent-color: var(--color-interactive-primary);
    margin-top: 2px;
  }

  .cat-label {
    display: block;
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-weight: var(--font-weight-medium);
  }

  .cat-desc {
    display: block;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .test-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .test-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .test-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
