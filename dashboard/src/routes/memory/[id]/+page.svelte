<script lang="ts">
  import { page } from '$app/stores';
  import { getGhostClient } from '$lib/ghost-client';
  import type { MemoryEntry } from '@ghost/sdk';

  let memoryId = $derived($page.params.id ?? '');
  let memory = $state<MemoryEntry | null>(null);
  let loading = $state(true);
  let error = $state('');
  let actionError = $state('');
  let notice = $state('');
  let actionLoading = $state(false);
  let archiveReason = $state('Archived from ADE memory detail');

  let snapshot = $derived(parseSnapshot(memory?.snapshot ?? '{}'));
  let isArchived = $derived(Boolean(snapshot.archived));

  $effect(() => {
    if (memoryId) {
      void loadMemory();
    }
  });

  async function loadMemory() {
    loading = true;
    error = '';

    try {
      const client = await getGhostClient();
      memory = await client.memory.get(memoryId);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load memory';
    } finally {
      loading = false;
    }
  }

  async function archiveMemory() {
    actionLoading = true;
    actionError = '';
    notice = '';

    try {
      const client = await getGhostClient();
      await client.memory.archive(memoryId, { reason: archiveReason });
      notice = 'Memory archived.';
      await loadMemory();
    } catch (e: unknown) {
      actionError = e instanceof Error ? e.message : 'Failed to archive memory';
    } finally {
      actionLoading = false;
    }
  }

  async function unarchiveMemory() {
    actionLoading = true;
    actionError = '';
    notice = '';

    try {
      const client = await getGhostClient();
      await client.memory.unarchive(memoryId);
      notice = 'Memory restored.';
      await loadMemory();
    } catch (e: unknown) {
      actionError = e instanceof Error ? e.message : 'Failed to restore memory';
    } finally {
      actionLoading = false;
    }
  }

  type SnapshotValue = string | number | boolean | null | SnapshotObject | SnapshotValue[];
  type SnapshotObject = Record<string, SnapshotValue>;

  function isSnapshotObject(value: unknown): value is SnapshotObject {
    return typeof value === 'object' && value !== null && !Array.isArray(value);
  }

  function parseSnapshot(raw: string | Record<string, unknown>): SnapshotObject {
    if (typeof raw !== 'string') return raw;
    try {
      const parsed = JSON.parse(raw);
      return isSnapshotObject(parsed) ? parsed : { raw };
    } catch {
      return { raw };
    }
  }

  function prettySnapshot(): string {
    return JSON.stringify(snapshot, null, 2);
  }

  function detailValue(value: unknown): string {
    if (value === null || value === undefined || value === '') return 'unavailable';
    return String(value);
  }
</script>

{#if loading}
  <div class="empty-state">Loading memory...</div>
{:else if error}
  <div class="empty-state">
    <p>{error}</p>
    <a href="/memory">Back to memory</a>
  </div>
{:else if memory}
  <div class="detail-page">
    <div class="detail-header">
      <a href="/memory" class="back-link">← Memory</a>
      <div class="detail-header__main">
        <div>
          <p class="eyebrow">Memory Detail</p>
          <h1>{detailValue(snapshot.summary ?? snapshot.memory_type ?? 'Memory')}</h1>
          <p class="subtitle mono">{memory.memory_id}</p>
        </div>
        <span class:archived={isArchived} class="badge">{isArchived ? 'Archived' : 'Active'}</span>
      </div>
    </div>

    {#if notice}
      <div class="banner notice">{notice}</div>
    {/if}

    {#if actionError}
      <div class="banner error">{actionError}</div>
    {/if}

    <div class="detail-grid">
      <section class="card">
        <h2>Metadata</h2>
        <dl class="meta-list">
          <dt>Type</dt><dd>{detailValue(snapshot.memory_type)}</dd>
          <dt>Importance</dt><dd>{detailValue(snapshot.importance)}</dd>
          <dt>Confidence</dt><dd>{detailValue(snapshot.confidence)}</dd>
          <dt>Created</dt><dd>{new Date(memory.created_at).toLocaleString()}</dd>
          <dt>Archived</dt><dd>{isArchived ? 'Yes' : 'No'}</dd>
        </dl>
      </section>

      <section class="card">
        <h2>Lifecycle</h2>
        {#if isArchived}
          <p class="helper">This memory is excluded from list and search surfaces unless archived entries are explicitly requested.</p>
          <button disabled={actionLoading} onclick={unarchiveMemory}>
            {actionLoading ? 'Working...' : 'Unarchive'}
          </button>
        {:else}
          <label class="archive-form">
            <span>Archive reason</span>
            <input type="text" bind:value={archiveReason} />
          </label>
          <button class="secondary" disabled={actionLoading} onclick={archiveMemory}>
            {actionLoading ? 'Working...' : 'Archive'}
          </button>
        {/if}
      </section>

      <section class="card wide">
        <h2>Snapshot</h2>
        <pre class="content-json">{prettySnapshot()}</pre>
      </section>
    </div>
  </div>
{/if}

<style>
  .detail-page {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-4);
  }

  .detail-header {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .detail-header__main {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-4);
    align-items: start;
  }

  .back-link {
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .eyebrow {
    margin: 0 0 var(--spacing-1);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  h1,
  h2 {
    margin: 0;
  }

  .subtitle {
    margin: var(--spacing-1) 0 0;
    color: var(--color-text-muted);
  }

  .mono {
    font-family: var(--font-family-mono);
  }

  .badge {
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-severity-normal) 18%, transparent);
    color: var(--color-severity-normal);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: var(--font-size-xs);
  }

  .badge.archived {
    background: color-mix(in srgb, var(--color-severity-soft) 18%, transparent);
    color: var(--color-text-primary);
  }

  .banner {
    padding: var(--spacing-3);
    border-radius: var(--radius-md);
    border: 1px solid transparent;
  }

  .banner.notice {
    border-color: color-mix(in srgb, var(--color-interactive-primary) 25%, transparent);
    background: color-mix(in srgb, var(--color-interactive-primary) 10%, transparent);
  }

  .banner.error {
    border-color: color-mix(in srgb, var(--color-severity-hard) 35%, transparent);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
  }

  .detail-grid {
    display: grid;
    gap: var(--spacing-4);
    grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    padding: var(--spacing-4);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
  }

  .card.wide {
    grid-column: 1 / -1;
  }

  .meta-list {
    display: grid;
    grid-template-columns: minmax(8rem, 10rem) 1fr;
    gap: var(--spacing-2);
    margin: 0;
  }

  .meta-list dt {
    color: var(--color-text-muted);
  }

  .meta-list dd {
    margin: 0;
    word-break: break-word;
  }

  .archive-form {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .archive-form span {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .archive-form input {
    padding: var(--spacing-2) var(--spacing-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
  }

  button {
    align-self: start;
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-interactive-primary);
    color: var(--color-text-inverse);
  }

  button.secondary {
    background: transparent;
    color: var(--color-text-primary);
  }

  .helper {
    margin: 0;
    color: var(--color-text-muted);
  }

  .content-json {
    margin: 0;
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    background: var(--color-bg-elevated-2);
    overflow: auto;
    font-family: var(--font-family-mono);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  .empty-state {
    padding: var(--spacing-8);
    text-align: center;
    border-radius: var(--radius-md);
    border: 1px dashed var(--color-border-default);
    color: var(--color-text-muted);
    background: var(--color-bg-elevated-1);
  }

  @media (max-width: 720px) {
    .detail-header__main {
      flex-direction: column;
    }
  }
</style>
