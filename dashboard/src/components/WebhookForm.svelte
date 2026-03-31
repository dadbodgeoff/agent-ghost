<script lang="ts">
  import type { WebhookEventType, WebhookSummary } from '@ghost/sdk';
  import { getGhostClient } from '$lib/ghost-client';

  interface Props {
    onSaved: () => void;
    editId?: string | null;
    initialWebhook?: WebhookSummary | null;
  }

  let { onSaved, editId = null, initialWebhook = null }: Props = $props();

  const EVENT_TYPES: WebhookEventType[] = [
    'intervention_change',
    'kill_switch',
    'proposal_decision',
    'agent_state_change',
    'score_update',
    'backup_complete',
  ];

  let name = $state('');
  let url = $state('');
  let secret = $state('');
  let selectedEvents = $state<WebhookEventType[]>([]);
  let saving = $state(false);
  let error = $state('');

  $effect(() => {
    name = initialWebhook?.name ?? '';
    url = initialWebhook?.url ?? '';
    secret = '';
    selectedEvents = [...(initialWebhook?.events ?? [])];
    error = '';
  });

  function toggleEvent(evt: WebhookEventType) {
    if (selectedEvents.includes(evt)) {
      selectedEvents = selectedEvents.filter(e => e !== evt);
    } else {
      selectedEvents = [...selectedEvents, evt];
    }
  }

  async function save() {
    if (!name.trim() || !url.trim()) {
      error = 'Name and URL are required';
      return;
    }
    if (!editId && !secret.trim()) {
      error = 'Secret is required for new webhooks';
      return;
    }
    if (selectedEvents.length === 0) {
      error = 'Select at least one event';
      return;
    }
    saving = true;
    error = '';
    try {
      const client = await getGhostClient();
      if (editId) {
        await client.webhooks.update(editId, { name, url, events: selectedEvents });
      } else {
        await client.webhooks.create({ name, url, secret, events: selectedEvents });
      }
      onSaved();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to save webhook';
    } finally {
      saving = false;
    }
  }
</script>

<form class="webhook-form" onsubmit={(e) => { e.preventDefault(); save(); }}>
  {#if error}
    <div class="form-error">{error}</div>
  {/if}

  <label class="field">
    <span>Name</span>
    <input type="text" bind:value={name} placeholder="e.g. Slack Alert" />
  </label>

  <label class="field">
    <span>URL</span>
    <input type="url" bind:value={url} placeholder="https://hooks.example.com/..." />
  </label>

  {#if !editId}
    <label class="field">
      <span>Secret (HMAC signing)</span>
      <input type="password" bind:value={secret} placeholder="Required shared secret" />
    </label>
  {/if}

  <fieldset class="events-field">
    <legend>Events</legend>
    <div class="event-grid">
      {#each EVENT_TYPES as evt}
        <label class="event-check">
          <input
            type="checkbox"
            checked={selectedEvents.includes(evt)}
            onchange={() => toggleEvent(evt)}
          />
          <span>{evt.replace(/_/g, ' ')}</span>
        </label>
      {/each}
    </div>
  </fieldset>

  <button type="submit" class="save-btn" disabled={saving}>
    {saving ? 'Saving...' : editId ? 'Update' : 'Create'} Webhook
  </button>
</form>

<style>
  .webhook-form {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-4);
    max-width: 520px;
  }

  .form-error {
    background: color-mix(in srgb, var(--color-severity-hard) 15%, transparent);
    color: var(--color-severity-hard);
    padding: var(--spacing-2) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .field span {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-weight: var(--font-weight-medium);
  }

  .field input {
    background: var(--color-bg-base);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border-default);
    padding: var(--spacing-2) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
  }

  .field input:focus {
    border-color: var(--color-interactive-primary);
    outline: none;
  }

  .events-field {
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-3);
  }

  .events-field legend {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-weight: var(--font-weight-medium);
    padding: 0 var(--spacing-1);
  }

  .event-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--spacing-2);
  }

  .event-check {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    cursor: pointer;
    text-transform: capitalize;
  }

  .event-check input[type="checkbox"] {
    accent-color: var(--color-interactive-primary);
  }

  .save-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
    align-self: flex-start;
  }

  .save-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .save-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
