<script lang="ts">
  /**
   * MemoryCard — displays a memory entry with parsed snapshot data.
   *
   * Props match the API shape: {memory_id, snapshot, created_at}.
   * Snapshot is a JSON string that gets parsed for display.
   * Ref: T-1.4.4, DESIGN_SYSTEM §8.4
   */

  let {
    memory_id = '',
    snapshot = '{}',
    created_at = '',
  }: {
    memory_id?: string;
    snapshot?: string;
    created_at?: string;
  } = $props();

  function parseSnapshot(raw: string): Record<string, any> {
    try {
      return JSON.parse(raw);
    } catch {
      return { content: raw };
    }
  }

  let data = $derived(parseSnapshot(snapshot));

  const IMPORTANCE_COLORS: Record<string, string> = {
    critical: 'var(--color-severity-hard)',
    high: 'var(--color-severity-active)',
    medium: 'var(--color-severity-soft)',
    low: 'var(--color-severity-normal)',
  };

  let importanceColor = $derived(
    IMPORTANCE_COLORS[(data.importance ?? '').toLowerCase()] ?? 'var(--color-text-muted)'
  );
</script>

<div class="memory-card">
  <div class="header">
    <span class="type">{data.memory_type ?? 'unknown'}</span>
    {#if data.importance}
      <span class="importance" style="color: {importanceColor}">{data.importance}</span>
    {/if}
  </div>
  <p class="content">{data.content ?? data.text ?? JSON.stringify(data).slice(0, 200)}</p>
  <div class="footer">
    <span class="id" title={memory_id}>{memory_id.slice(0, 8)}</span>
    {#if created_at}
      <span class="date">{new Date(created_at).toLocaleDateString()}</span>
    {/if}
  </div>
</div>

<style>
  .memory-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
  }

  .header {
    display: flex;
    justify-content: space-between;
    margin-bottom: var(--spacing-2);
    font-size: var(--font-size-sm);
  }

  .type {
    color: var(--color-brand-primary);
    font-weight: var(--font-weight-semibold);
  }

  .importance {
    font-weight: var(--font-weight-semibold);
  }

  .content {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    line-height: var(--line-height-normal);
    margin: 0;
    word-break: break-word;
  }

  .footer {
    display: flex;
    justify-content: space-between;
    margin-top: var(--spacing-2);
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
  }

  .id {
    font-family: var(--font-family-mono);
  }
</style>