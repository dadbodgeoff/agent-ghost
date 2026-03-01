<script lang="ts">
  /**
   * StatusBadge — agent/session status indicator.
   * Ref: T-X.6, DESIGN_SYSTEM §8.4
   */

  let {
    status = 'active',
  }: {
    status?: 'active' | 'paused' | 'quarantined' | 'deleted' | string;
  } = $props();

  const STATUS_MAP: Record<string, { color: string; bg: string; label: string }> = {
    active:       { color: 'var(--color-severity-normal)', bg: 'var(--color-severity-normal-bg)', label: 'Active' },
    paused:       { color: 'var(--color-severity-soft)',   bg: 'var(--color-severity-soft-bg)',   label: 'Paused' },
    quarantined:  { color: 'var(--color-severity-hard)',   bg: 'var(--color-severity-hard-bg)',   label: 'Quarantined' },
    deleted:      { color: 'var(--color-text-disabled)',   bg: 'var(--color-surface-disabled)',   label: 'Deleted' },
  };

  let info = $derived(
    STATUS_MAP[status] || { color: 'var(--color-text-muted)', bg: 'var(--color-bg-elevated-3)', label: status }
  );
</script>

<span class="badge" style="color: {info.color}; background: {info.bg}" role="status">
  <span class="dot" style="background: {info.color}" aria-hidden="true"></span>
  {info.label}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-1);
    padding: var(--spacing-0-5) var(--spacing-2);
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    line-height: var(--line-height-tight);
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-full);
    flex-shrink: 0;
  }
</style>