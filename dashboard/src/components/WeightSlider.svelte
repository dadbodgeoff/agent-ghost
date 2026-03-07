<script lang="ts">
  /**
   * WeightSlider — 8-signal weight editor that enforces sum-to-1.0 constraint.
   * Used in convergence profile editor.
   *
   * Ref: T-3.10.1, DESIGN_SYSTEM §8.4
   */

  const SIGNAL_NAMES = [
    'Session Duration',
    'Inter-Session Gap',
    'Response Latency',
    'Vocabulary Convergence',
    'Goal Boundary Erosion',
    'Initiative Balance',
    'Disengagement Resistance',
    'Behavioral Anomaly',
  ];

  interface Props {
    weights: number[];
    onchange?: (weights: number[]) => void;
    disabled?: boolean;
  }

  const DEFAULT_WEIGHTS = [0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125];
  let { weights = DEFAULT_WEIGHTS, onchange, disabled = false }: Props = $props();

  let localWeights = $state([...DEFAULT_WEIGHTS]);

  $effect(() => {
    localWeights = [...weights];
  });

  let sum = $derived(localWeights.reduce((a, b) => a + b, 0));
  let isValid = $derived(Math.abs(sum - 1.0) < 0.01);

  function handleSlider(index: number, event: Event) {
    const target = event.target as HTMLInputElement;
    const newVal = parseFloat(target.value);
    const diff = newVal - localWeights[index];

    // Distribute difference proportionally across other weights.
    const others = localWeights.filter((_, i) => i !== index);
    const othersSum = others.reduce((a, b) => a + b, 0);

    const updated = [...localWeights];
    updated[index] = newVal;

    if (othersSum > 0) {
      for (let i = 0; i < updated.length; i++) {
        if (i !== index) {
          updated[i] = Math.max(0, updated[i] - (diff * (localWeights[i] / othersSum)));
        }
      }
    }

    localWeights = updated;
    onchange?.(updated);
  }

  function resetEqual() {
    localWeights = Array(8).fill(1 / 8);
    onchange?.(localWeights);
  }
</script>

<div class="weight-slider-group">
  <div class="header">
    <span class="label">Signal Weights</span>
    <span class="sum" class:invalid={!isValid}>
      Sum: {sum.toFixed(3)}
    </span>
    <button class="reset-btn" onclick={resetEqual} {disabled}>Reset Equal</button>
  </div>

  {#each localWeights as weight, i}
    <div class="slider-row">
      <label class="signal-name" for="weight-{i}">{SIGNAL_NAMES[i]}</label>
      <input
        id="weight-{i}"
        type="range"
        min="0"
        max="0.5"
        step="0.005"
        value={weight}
        oninput={(e) => handleSlider(i, e)}
        {disabled}
        class="slider"
      />
      <span class="weight-value mono">{weight.toFixed(3)}</span>
    </div>
  {/each}

  {#if !isValid}
    <p class="validation-warning">Weights must sum to 1.0 (currently {sum.toFixed(3)})</p>
  {/if}
</div>

<style>
  .weight-slider-group {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--spacing-4);
  }

  .label {
    font-weight: 600;
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .sum {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-sm);
    color: var(--color-severity-normal);
    font-variant-numeric: tabular-nums;
  }

  .sum.invalid {
    color: var(--color-severity-hard);
  }

  .reset-btn {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-2);
    font-size: var(--font-size-xs);
    cursor: pointer;
    color: var(--color-text-secondary);
  }

  .reset-btn:hover:not(:disabled) {
    background: var(--color-bg-elevated-3);
  }

  .slider-row {
    display: grid;
    grid-template-columns: 180px 1fr 60px;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-1) 0;
  }

  .signal-name {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .slider {
    width: 100%;
    accent-color: var(--color-interactive-primary);
  }

  .weight-value {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .validation-warning {
    margin-top: var(--spacing-2);
    color: var(--color-severity-hard);
    font-size: var(--font-size-xs);
  }

  .mono {
    font-family: var(--font-family-mono);
  }
</style>
