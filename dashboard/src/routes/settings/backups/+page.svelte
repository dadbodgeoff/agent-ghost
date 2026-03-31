<script lang="ts">
  /**
   * Backup management — list, create, restore backups.
   *
   * Ref: T-3.12.1
   */
  import { onMount } from 'svelte';
  import type { Backup } from '@ghost/sdk';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';

  let backups: Backup[] = $state([]);
  let loading = $state(true);
  let creating = $state(false);
  let error: string | null = $state(null);
  let success: string | null = $state(null);

  onMount(() => {
    loadBackups();

    // T-5.9.1: Wire BackupComplete WS event to refresh backup list.
    const unsub = wsStore.on('BackupComplete', () => { loadBackups(); });
    const unsubResync = wsStore.onResync(() => { loadBackups(); });
    return () => {
      unsub();
      unsubResync();
    };
  });

  async function loadBackups() {
    loading = true;
    error = null;
    try {
      const client = await getGhostClient();
      const res = await client.backups.list();
      backups = res.backups ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load backups';
    } finally {
      loading = false;
    }
  }

  async function createBackup() {
    creating = true;
    error = null;
    success = null;
    try {
      const client = await getGhostClient();
      const res = await client.backups.create();
      success = `Backup created: ${res.backup_id.slice(0, 8)}… (${formatBytes(res.size_bytes)})`;
      await loadBackups();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to create backup';
    } finally {
      creating = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(1)} MB`;
  }

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleString();
  }

  let lastBackupAge = $derived.by(() => {
    if (backups.length === 0) return null;
    const last = new Date(backups[0].created_at);
    const hours = Math.floor((Date.now() - last.getTime()) / 3600000);
    if (hours < 1) return 'less than 1 hour ago';
    if (hours < 24) return `${hours} hours ago`;
    return `${Math.floor(hours / 24)} days ago`;
  });
</script>

<svelte:head>
  <title>Backups | Settings | ADE</title>
</svelte:head>

<div class="backups-page">
  <header class="page-header">
    <div class="header-row">
      <div>
        <h1>Backups</h1>
        <p class="subtitle">Point-in-time backup management with BLAKE3 integrity verification</p>
      </div>
      <button class="backup-btn" onclick={createBackup} disabled={creating}>
        {creating ? 'Creating…' : 'Backup Now'}
      </button>
    </div>
  </header>

  {#if error}
    <p class="msg error-msg">{error}</p>
  {/if}
  {#if success}
    <p class="msg success-msg">{success}</p>
  {/if}

  {#if lastBackupAge}
    <div class="last-backup-indicator">
      Last backup: {lastBackupAge}
    </div>
  {/if}

  {#if loading}
    <p class="loading">Loading backups…</p>
  {:else if backups.length === 0}
    <p class="empty">No backups found. Click "Backup Now" to create the first backup.</p>
  {:else}
    <div class="table-wrap">
      <table class="data-table">
      <thead>
        <tr>
          <th>ID</th>
          <th>Created</th>
          <th>Size</th>
          <th>Entries</th>
          <th>Status</th>
          <th>Checksum</th>
        </tr>
      </thead>
      <tbody>
        {#each backups as backup}
          <tr>
            <td class="mono">{backup.backup_id.slice(0, 8)}…</td>
            <td>{formatDate(backup.created_at)}</td>
            <td class="mono">{formatBytes(backup.size_bytes)}</td>
            <td class="mono">{backup.entry_count}</td>
            <td>
              <span class="status-badge" class:complete={backup.status === 'complete'}>
                {backup.status}
              </span>
            </td>
            <td class="mono checksum">{backup.blake3_checksum.slice(0, 16)}…</td>
          </tr>
        {/each}
      </tbody>
    </table>
    </div>
  {/if}
</div>

<style>
  .backups-page { padding: var(--spacing-6); max-width: 1000px; }
  .page-header { margin-bottom: var(--spacing-6); }
  .header-row { display: flex; justify-content: space-between; align-items: flex-start; }
  .page-header h1 { font-size: var(--font-size-2xl); font-weight: 700; color: var(--color-text-primary); }
  .subtitle { color: var(--color-text-muted); font-size: var(--font-size-sm); margin-top: var(--spacing-1); }

  .backup-btn {
    background: var(--color-interactive-primary);
    color: var(--color-text-inverse);
    border: none;
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-4);
    cursor: pointer;
    font-size: var(--font-size-sm);
    font-weight: 500;
  }
  .backup-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .last-backup-indicator {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-3);
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    margin-bottom: var(--spacing-4);
  }

  .table-wrap { overflow-x: auto; }
  .data-table { width: 100%; min-width: 720px; border-collapse: collapse; font-size: var(--font-size-sm); }
  .data-table th {
    text-align: left; padding: var(--spacing-2) var(--spacing-3); background: var(--color-bg-elevated-1);
    color: var(--color-text-muted); font-weight: 600; font-size: var(--font-size-xs);
    text-transform: uppercase; border-bottom: 1px solid var(--color-border-default);
  }
  .data-table td { padding: var(--spacing-2) var(--spacing-3); border-bottom: 1px solid var(--color-border-subtle); color: var(--color-text-primary); }

  .status-badge {
    display: inline-block; padding: 2px 8px; border-radius: var(--radius-sm);
    font-size: var(--font-size-xs); background: var(--color-bg-elevated-2); color: var(--color-text-muted);
  }
  .status-badge.complete { background: var(--color-severity-normal); color: var(--color-text-inverse); }

  .checksum { font-size: var(--font-size-xs); color: var(--color-text-muted); }
  .msg { padding: var(--spacing-2) var(--spacing-3); border-radius: var(--radius-sm); font-size: var(--font-size-sm); margin-bottom: var(--spacing-3); }
  .error-msg { background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-hard); color: var(--color-severity-hard); }
  .success-msg { background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-normal); color: var(--color-severity-normal); }
  .empty, .loading { text-align: center; padding: var(--spacing-8); color: var(--color-text-muted); font-size: var(--font-size-sm); }
  .mono { font-family: var(--font-family-mono); font-variant-numeric: tabular-nums; }
</style>
