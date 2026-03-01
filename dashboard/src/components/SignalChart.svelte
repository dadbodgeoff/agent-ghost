<script lang="ts">
  /**
   * SignalChart — horizontal bar chart for 7 convergence signals.
   * Ref: T-1.9.2, DESIGN_SYSTEM §8.4
   */

  let {
    signals = [0, 0, 0, 0, 0, 0, 0],
  }: {
    signals?: number[];
  } = $props();

  const SIGNAL_NAMES = [
    'Session Duration', 'Inter-Session Gap', 'Response Latency',
    'Vocabulary Convergence', 'Goal Boundary Erosion',
    'Initiative Balance', 'Disengagement Resistance',
  ];

  function barColor(val: number): string {
    if (val < 0.3) return 'var(--color-severity-normal)';
    if (val < 0.5) return 'var(--color-severity-soft)';
    if (val < 0.7) return 'var(--color-severity-active)';
    return 'var(--color-severity-hard)';
  }
</script>

<div class="signal-chart" role="list" aria-label="Convergence signals">
  {#each signals as val, i}
    <div class="signal-row" role="listitem">
      <span class="name">{SIGNAL_NAMES[i]}</span>
      <div class="bar-container" aria-hidden="true">
        <div class="bar" style="width: {val * 100}%; background: {barColor(val)}"></div>
      </div>
      <span class="value">{val.toFixed(3)}</span>
    </div>
  {/each}
</div>

<style>
  .signal-chart {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .signal-row {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    font-size: var(--font-size-sm);
  }

  .name {
    width: 180px;
    color: var(--color-text-secondary);
    flex-shrink: 0;
    font-size: var(--font-size-sm);
  }

  .bar-container {
    flex: 1;
    height: 6px;
    background: var(--color-border-default);
    border-radius: var(--radius-full);
    overflow: hidden;
  }

  .bar {
    height: 100%;
    border-radius: var(--radius-full);
    transition: width var(--duration-slow) var(--easing-default);
  }

  .value {
    width: 50px;
    text-align: right;
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
  }
</style>