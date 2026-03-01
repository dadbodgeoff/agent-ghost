<script lang="ts">
  /**
   * ScoreGauge — semicircular convergence score visualization.
   * Maps intervention level (L0–L4) to severity color tokens.
   * Ref: T-1.9.1, DESIGN_SYSTEM §8.4
   */

  let {
    score = 0,
    level = 0,
  }: {
    score?: number;
    level?: number;
  } = $props();

  const LEVEL_LABELS = ['Normal', 'Soft', 'Active', 'Hard', 'External'];
  const LEVEL_CSS_VARS = [
    'var(--color-severity-normal)',
    'var(--color-severity-soft)',
    'var(--color-severity-active)',
    'var(--color-severity-hard)',
    'var(--color-severity-external)',
  ];

  let color = $derived(LEVEL_CSS_VARS[level] || LEVEL_CSS_VARS[0]);
  let label = $derived(LEVEL_LABELS[level] || 'Unknown');
</script>

<div
  class="gauge"
  role="meter"
  aria-valuenow={score}
  aria-valuemin={0}
  aria-valuemax={1}
  aria-label="Convergence score: {score.toFixed(2)}, Level {level} {label}"
>
  <svg viewBox="0 0 200 120" class="gauge-svg">
    <path
      d="M 20 100 A 80 80 0 0 1 180 100"
      fill="none"
      stroke="var(--color-border-default)"
      stroke-width="12"
      stroke-linecap="round"
    />
    <path
      d="M 20 100 A 80 80 0 0 1 180 100"
      fill="none"
      stroke={color}
      stroke-width="12"
      stroke-linecap="round"
      stroke-dasharray="{score * 251.2} 251.2"
    />
  </svg>
  <div class="score-value" style="color: {color}">{score.toFixed(2)}</div>
  <div class="level-badge" style="background: color-mix(in srgb, {color} 15%, transparent); color: {color}">
    L{level} — {label}
  </div>
</div>

<style>
  .gauge {
    text-align: center;
    padding: var(--spacing-4) 0;
  }

  .gauge-svg {
    width: 160px;
    height: 100px;
  }

  .score-value {
    font-size: var(--font-size-2xl);
    font-weight: var(--font-weight-bold);
    font-variant-numeric: tabular-nums;
    line-height: var(--line-height-tight);
    margin-top: calc(-1 * var(--spacing-2));
  }

  .level-badge {
    display: inline-block;
    padding: var(--spacing-0-5) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    margin-top: var(--spacing-2);
  }
</style>