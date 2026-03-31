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
    focused = false,
  }: {
    memory_id?: string;
    snapshot?: string;
    created_at?: string;
    focused?: boolean;
  } = $props();

  function parseSnapshot(raw: string): JsonObject {
    try {
      const parsed = JSON.parse(raw);
      return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
        ? (parsed as JsonObject)
        : { content: raw };
    } catch {
      return { content: raw };
    }
  }

  let data = $derived(parseSnapshot(snapshot));

  const IMPORTANCE_COLORS: Record<string, string> = {
    critical: 'var(--color-severity-hard)',
    high: 'var(--color-severity-active)',
    normal: 'var(--color-severity-soft)',
    medium: 'var(--color-severity-soft)',
    low: 'var(--color-severity-normal)',
  };

  let importanceColor = $derived(
    IMPORTANCE_COLORS[(data.importance ?? '').toLowerCase()] ?? 'var(--color-text-muted)'
  );

  function readNestedString(value: JsonValue | undefined, keys: string[]): string | null {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
    for (const key of keys) {
      const candidate = value[key];
      if (typeof candidate === 'string' && candidate.trim()) return candidate;
    }
    return null;
  }

  function previewContent(data: JsonObject): string {
    if (typeof data.summary === 'string' && data.summary.trim()) return data.summary;
    if (typeof data.content === 'string' && data.content.trim()) return data.content;
    if (data.content && typeof data.content === 'object' && !Array.isArray(data.content)) {
      const nested = readNestedString(data.content, ['goal_text', 'text', 'message', 'description', 'fact']);
      if (nested) return nested;
      return JSON.stringify(data.content).slice(0, 200);
    }
    if (typeof data.text === 'string' && data.text.trim()) return data.text;
    return JSON.stringify(data).slice(0, 200);
  }
</script>

<div class="memory-card" class:focused id={`memory-${memory_id}`}>
  <div class="header">
    <span class="type">{data.memory_type ?? 'unknown'}</span>
    {#if data.importance}
      <span class="importance" style="color: {importanceColor}">{data.importance}</span>
    {/if}
  </div>
  <p class="content">{previewContent(data)}</p>
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

  .memory-card.focused {
    border-color: var(--color-interactive-primary);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-interactive-primary) 45%, transparent);
    background: color-mix(in srgb, var(--color-interactive-primary) 7%, var(--color-bg-elevated-1));
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
  import type { JsonObject, JsonValue } from '$lib/types/json';
