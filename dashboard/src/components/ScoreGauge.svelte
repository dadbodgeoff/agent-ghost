<script lang="ts">
  export let score: number = 0;
  export let level: number = 0;

  const LEVEL_LABELS = ['Normal', 'Soft', 'Active', 'Hard', 'External'];
  const LEVEL_COLORS = ['#22c55e', '#eab308', '#f97316', '#ef4444', '#991b1b'];

  $: color = LEVEL_COLORS[level] || LEVEL_COLORS[0];
  $: label = LEVEL_LABELS[level] || 'Unknown';
  $: rotation = score * 180;
</script>

<div class="gauge" role="meter" aria-valuenow={score} aria-valuemin={0} aria-valuemax={1} aria-label="Convergence score">
  <svg viewBox="0 0 200 120" class="gauge-svg">
    <path d="M 20 100 A 80 80 0 0 1 180 100" fill="none" stroke="#27272a" stroke-width="12" stroke-linecap="round" />
    <path d="M 20 100 A 80 80 0 0 1 180 100" fill="none" stroke={color} stroke-width="12" stroke-linecap="round"
      stroke-dasharray="{score * 251.2} 251.2" />
  </svg>
  <div class="score-value" style="color: {color}">{score.toFixed(2)}</div>
  <div class="level-badge" style="background: {color}20; color: {color}">Level {level} — {label}</div>
</div>

<style>
  .gauge { text-align: center; padding: 16px 0; }
  .gauge-svg { width: 160px; height: 100px; }
  .score-value { font-size: 36px; font-weight: 700; font-variant-numeric: tabular-nums; margin-top: -8px; }
  .level-badge { display: inline-block; padding: 2px 10px; border-radius: 4px; font-size: 12px; font-weight: 600; margin-top: 8px; }
</style>
