<script lang="ts">
  export let signals: number[] = [0, 0, 0, 0, 0, 0, 0];

  const SIGNAL_NAMES = [
    'Session Duration', 'Inter-Session Gap', 'Response Latency',
    'Vocabulary Convergence', 'Goal Boundary Erosion',
    'Initiative Balance', 'Disengagement Resistance',
  ];

  function barColor(val: number): string {
    if (val < 0.3) return '#22c55e';
    if (val < 0.5) return '#eab308';
    if (val < 0.7) return '#f97316';
    return '#ef4444';
  }
</script>

<div class="signal-chart" role="list" aria-label="Convergence signals">
  {#each signals as val, i}
    <div class="signal-row" role="listitem">
      <span class="name">{SIGNAL_NAMES[i]}</span>
      <div class="bar-container">
        <div class="bar" style="width: {val * 100}%; background: {barColor(val)}"></div>
      </div>
      <span class="value">{val.toFixed(3)}</span>
    </div>
  {/each}
</div>

<style>
  .signal-chart { display: flex; flex-direction: column; gap: 8px; }
  .signal-row { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .name { width: 180px; color: #a1a1aa; flex-shrink: 0; }
  .bar-container { flex: 1; height: 6px; background: #27272a; border-radius: 3px; overflow: hidden; }
  .bar { height: 100%; border-radius: 3px; transition: width 0.3s; }
  .value { width: 50px; text-align: right; font-variant-numeric: tabular-nums; }
</style>
