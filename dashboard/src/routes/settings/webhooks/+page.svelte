<script lang="ts">
  import { onMount } from 'svelte';
  import type { WebhookSummary } from '@ghost/sdk';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import WebhookForm from '../../../components/WebhookForm.svelte';

  let webhooks = $state<WebhookSummary[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let showForm = $state(false);
  let editId = $state<string | null>(null);
  let editWebhookData = $state<WebhookSummary | null>(null);
  let testingId = $state<string | null>(null);
  let testResult = $state<{ id: string; success: boolean; code: number } | null>(null);

  onMount(() => {
    void loadWebhooks();

    // T-5.9.1: Wire WebhookFired WS event to refresh webhook list.
    const unsub = wsStore.on('WebhookFired', () => { void loadWebhooks(); });
    const unsubResync = wsStore.onResync(() => { void loadWebhooks(); });
    return () => {
      unsub();
      unsubResync();
    };
  });

  async function loadWebhooks() {
    loading = true;
    error = null;
    try {
      const client = await getGhostClient();
      const data = await client.webhooks.list();
      webhooks = data.webhooks ?? [];
    } catch (e: unknown) {
      // T-5.9.2: Show error instead of swallowing.
      error = e instanceof Error ? e.message : 'Failed to load webhooks';
    } finally {
      loading = false;
    }
  }

  async function deleteWebhook(id: string) {
    if (!confirm('Delete this webhook?')) return;
    error = null;
    try {
      const client = await getGhostClient();
      await client.webhooks.delete(id);
      await loadWebhooks();
    } catch (e: unknown) {
      // T-5.9.2: Show error instead of swallowing.
      error = e instanceof Error ? e.message : 'Failed to delete webhook';
    }
  }

  async function testWebhook(id: string) {
    testingId = id;
    testResult = null;
    error = null;
    try {
      const client = await getGhostClient();
      const data = await client.webhooks.test(id);
      testResult = { id, success: data.success, code: data.status_code };
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Webhook test failed';
      testResult = { id, success: false, code: 0 };
    } finally {
      testingId = null;
    }
  }

  function onFormSaved() {
    showForm = false;
    editId = null;
    editWebhookData = null;
    void loadWebhooks();
  }

  function editWebhook(wh: WebhookSummary) {
    editId = wh.id;
    editWebhookData = wh;
    showForm = true;
  }
</script>

<div class="page">
  <header class="page-header">
    <div>
      <h1>Webhooks</h1>
      <p class="subtitle">Configure webhook endpoints for event notifications</p>
    </div>
    <button
      class="add-btn"
      onclick={() => {
        showForm = !showForm;
        if (!showForm) {
          editId = null;
          editWebhookData = null;
        }
      }}
    >
      {showForm ? 'Cancel' : '+ New Webhook'}
    </button>
  </header>

  {#if showForm}
    <div class="form-container">
      <WebhookForm onSaved={onFormSaved} {editId} initialWebhook={editWebhookData} />
    </div>
  {/if}

  {#if error}
    <div class="error-banner">
      <p>{error}</p>
      <button onclick={() => { error = null; loadWebhooks(); }}>Retry</button>
    </div>
  {/if}

  {#if loading}
    <p class="loading">Loading webhooks...</p>
  {:else if webhooks.length === 0 && !showForm}
    <div class="empty-state">
      <p>No webhooks configured. Create one to receive event notifications.</p>
    </div>
  {:else}
    <div class="webhook-list">
      {#each webhooks as wh (wh.id)}
        <div class="webhook-row">
          <div class="webhook-info">
            <div class="webhook-header">
              <span class="webhook-name">{wh.name}</span>
              <span class="webhook-status" class:active={wh.active}>
                {wh.active ? 'Active' : 'Inactive'}
              </span>
            </div>
            <span class="webhook-url">{wh.url}</span>
            <div class="webhook-events">
              {#each wh.events as evt}
                <span class="event-tag">{evt.replace(/_/g, ' ')}</span>
              {/each}
            </div>
          </div>
          <div class="webhook-actions">
            <button class="edit-btn" onclick={() => editWebhook(wh)}>Edit</button>
            <button
              class="test-btn"
              disabled={testingId === wh.id}
              onclick={() => testWebhook(wh.id)}
            >
              {testingId === wh.id ? '...' : 'Test'}
            </button>
            {#if testResult && testResult.id === wh.id}
              <span class="test-result" class:success={testResult.success}>
                {testResult.success ? `OK (${testResult.code})` : `Failed (${testResult.code})`}
              </span>
            {/if}
            <button class="delete-btn" onclick={() => deleteWebhook(wh.id)}>Delete</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-6);
  }

  .page-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
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

  .add-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .form-container {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-5);
  }

  .loading {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .empty-state {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .webhook-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .webhook-row {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--spacing-4);
  }

  .webhook-info {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    flex: 1;
    min-width: 0;
  }

  .webhook-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
  }

  .webhook-name {
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .webhook-status {
    font-size: var(--font-size-xs);
    padding: 1px var(--spacing-2);
    border-radius: var(--radius-full);
    background: color-mix(in srgb, var(--color-text-muted) 15%, transparent);
    color: var(--color-text-muted);
  }

  .webhook-status.active {
    background: color-mix(in srgb, var(--color-score-high) 15%, transparent);
    color: var(--color-score-high);
  }

  .webhook-url {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .webhook-events {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-1);
  }

  .event-tag {
    font-size: var(--font-size-xs);
    padding: 1px var(--spacing-2);
    border-radius: var(--radius-sm);
    background: var(--color-surface-selected);
    color: var(--color-text-secondary);
    text-transform: capitalize;
  }

  .webhook-actions {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    flex-shrink: 0;
  }

  .edit-btn {
    background: transparent;
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-default);
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .edit-btn:hover {
    border-color: var(--color-brand-primary);
    color: var(--color-brand-primary);
  }

  .test-btn {
    background: transparent;
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-default);
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .test-btn:hover:not(:disabled) {
    border-color: var(--color-border-emphasis);
    color: var(--color-text-primary);
  }

  .test-result {
    font-size: var(--font-size-xs);
    color: var(--color-severity-hard);
    font-variant-numeric: tabular-nums;
  }

  .test-result.success {
    color: var(--color-score-high);
  }

  .delete-btn {
    background: transparent;
    color: var(--color-severity-hard);
    border: 1px solid var(--color-severity-hard);
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .delete-btn:hover {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }
</style>
