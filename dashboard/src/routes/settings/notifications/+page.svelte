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

  function decodeApplicationServerKey(key: string): Uint8Array {
    const padding = '='.repeat((4 - (key.length % 4)) % 4);
    const normalized = (key + padding).replace(/-/g, '+').replace(/_/g, '/');
    const raw = atob(normalized);
    return Uint8Array.from(raw, (char) => char.charCodeAt(0));
  }

  onMount(async () => {
    pushSupported = 'PushManager' in window && 'Notification' in window;
    if (pushSupported) {
      permissionState = Notification.permission;
      pushEnabled = permissionState === 'granted';

      // Load saved preferences.
      const saved = localStorage.getItem('ghost-push-categories');
      if (saved) {
        try {
          enabledCategories = JSON.parse(saved);
        } catch { /* use defaults */ }
      }
    }
  });

  async function togglePush() {
    if (!pushSupported) return;

    if (!pushEnabled) {
      const permission = await Notification.requestPermission();
      permissionState = permission;
      if (permission === 'granted') {
        pushEnabled = true;
        await subscribePush();
      }
    } else {
      pushEnabled = false;
      await unsubscribePush();
    }
  }

  async function subscribePush() {
    try {
      const client = await getGhostClient();
      const reg = await navigator.serviceWorker.ready;
      const keyData = await client.push.getVapidKey();
      if (!keyData.key) return;

      const sub = await reg.pushManager.subscribe({
        userVisibleOnly: true,
        applicationServerKey: decodeApplicationServerKey(keyData.key),
      });
      await client.push.subscribe(sub.toJSON());
    } catch {
      // Push subscription failed.
    }
  }

  async function unsubscribePush() {
    try {
      const client = await getGhostClient();
      const reg = await navigator.serviceWorker.ready;
      const sub = await reg.pushManager.getSubscription();
      if (sub) {
        await client.push.unsubscribe(sub.toJSON());
        await sub.unsubscribe();
      }
    } catch {
      // Unsubscribe failed.
    }
  }

  function toggleCategory(id: string) {
    if (enabledCategories.includes(id)) {
      enabledCategories = enabledCategories.filter(c => c !== id);
    } else {
      enabledCategories = [...enabledCategories, id];
    }
    localStorage.setItem('ghost-push-categories', JSON.stringify(enabledCategories));
  }

  async function sendTestNotification() {
    testSending = true;
    try {
      const reg = await navigator.serviceWorker.ready;
      await reg.showNotification('GHOST Test', {
        body: 'Push notifications are working correctly.',
        icon: '/icons/ghost-192.png',
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

  .unsupported {
    background: color-mix(in srgb, var(--color-severity-active) 15%, transparent);
    color: var(--color-severity-active);
    padding: var(--spacing-3) var(--spacing-4);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
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
