<script lang="ts">
  /**
   * CostBar — horizontal utilization bar with cap indicator.
   *
   * Accepts either `used`+`cap` (computes %) or `utilization` (direct %).
   * Ref: T-X.8, DESIGN_SYSTEM §8.4
   */

  let {
    used = 0,
    cap = 10,
    utilization,
    label = '',
  }: {
    used?: number;
    cap?: number;
    utilization?: number;
    label?: string;
  } = $props();

  let pct = $derived(
    utilization != null
      ? Math.min(100, utilization)
      : cap > 0
        ? Math.min(100, (used / cap) * 100)
        : 0
  );

  let color = $derived(
    pct >= 95
      ? 'var(--color-severity-hard)'
      : pct >= 80
        ? 'var(--color-severity-soft)'
        : 'var(--color-brand-primary)'
  );

  let showLabel = $derived(label !== '' && utilization == null);
</script>

<div class="cost-bar">
  {#if showLabel}
    <div class="label-row">
      <span class="label">{label}</span>
      <span class="value">${used.toFixed(2)} / ${cap.toFixed(2)}</span>
    </div>
  {/if}
  <div
    class="track"
    role="progressbar"
    aria-valuenow={pct}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-label="{label || 'Cost'} utilization: {pct.toFixed(0)}%"
  >
    <div class="fill" style="width: {pct}%; background: {color}"></div>
  </div>
</div>

<style>
  .cost-bar {
    width: 100%;
  }

  .label-row {
    display: flex;
    justify-content: space-between;
    margin-bottom: var(--spacing-1);
    font-size: var(--font-size-xs);
  }

  .label {
    color: var(--color-text-muted);
  }

  .value {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    color: var(--color-text-secondary);
  }

  .track {
    height: 6px;
    background: var(--color-border-default);
    border-radius: var(--radius-full);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: var(--radius-full);
    transition: width var(--duration-normal) var(--easing-default);
  }
</style>