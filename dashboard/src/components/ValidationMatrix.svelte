<!--
  ValidationMatrix — 7-dimension validation grid (T-X.12).

  Displays the D1–D7 validation dimension scores for a proposal:
  D1: Base Quality, D2: Safety, D3: Relevance, D4: Consistency,
  D5: Scope Expansion, D6: Self-Reference, D7: Emulation Language.

  Each dimension shows a score bar and pass/fail status.

  Ref: ADE_DESIGN_PLAN §7, tasks.md T-X.12
-->
<script lang="ts">
  import type { JsonObject } from '$lib/types/json';

  interface DimensionScore {
    dimension: string;
    label: string;
    score: number;
    threshold: number;
    passed: boolean;
  }

  interface ScoreValue {
    score?: number;
    threshold?: number;
    passed?: boolean;
  }

  interface Props {
    scores?: Record<string, number | ScoreValue | JsonObject>;
    compact?: boolean;
  }

  let { scores = {}, compact = false }: Props = $props();

  const dimensionLabels: Record<string, string> = {
    d1: 'Base Quality',
    d2: 'Safety',
    d3: 'Relevance',
    d4: 'Consistency',
    d5: 'Scope Expansion',
    d6: 'Self-Reference',
    d7: 'Emulation Language',
  };

  let dimensions = $derived(
    Object.entries(dimensionLabels).map(([key, label]) => {
      const raw = scores[key] ?? scores[key.toUpperCase()] ?? {};
      const value = typeof raw === 'number' ? { score: raw } : raw;
      const score = typeof value.score === 'number' ? value.score : 0;
      const threshold = typeof value.threshold === 'number' ? value.threshold : 0.5;
      const passed = typeof value.passed === 'boolean' ? value.passed : score >= threshold;
      return { dimension: key, label, score, threshold, passed } satisfies DimensionScore;
    })
  );

  let overallPass = $derived(dimensions.every(d => d.passed));
</script>

<div class="validation-matrix" class:compact role="table" aria-label="Validation dimensions">
  <div class="matrix-header" role="row">
    <span class="header-dim" role="columnheader">Dimension</span>
    <span class="header-score" role="columnheader">Score</span>
    <span class="header-status" role="columnheader">Status</span>
  </div>

  {#each dimensions as dim}
    <div class="matrix-row" role="row" class:failed={!dim.passed}>
      <span class="row-dim" role="cell">
        <span class="dim-key">{dim.dimension.toUpperCase()}</span>
        {#if !compact}
          <span class="dim-label">{dim.label}</span>
        {/if}
      </span>
      <span class="row-score" role="cell">
        <div class="score-bar">
          <div
            class="score-fill"
            style="width: {Math.min(dim.score * 100, 100)}%"
            style:background={dim.passed ? 'var(--color-severity-normal)' : 'var(--color-severity-hard)'}
          ></div>
          <div
            class="threshold-mark"
            style="left: {dim.threshold * 100}%"
          ></div>
        </div>
        <span class="score-value">{dim.score.toFixed(2)}</span>
      </span>
      <span class="row-status" role="cell">
        <span class="status-badge" class:pass={dim.passed} class:fail={!dim.passed}>
          {dim.passed ? 'PASS' : 'FAIL'}
        </span>
      </span>
    </div>
  {/each}

  <div class="matrix-footer">
    <span class="overall" class:pass={overallPass} class:fail={!overallPass}>
      Overall: {overallPass ? 'PASS' : 'FAIL'}
    </span>
  </div>
</div>

<style>
  .validation-matrix {
    display: flex;
    flex-direction: column;
    gap: 0;
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    overflow: hidden;
    font-family: var(--font-family-mono);
    font-size: var(--font-size-sm);
  }

  .matrix-header {
    display: grid;
    grid-template-columns: 1fr 2fr auto;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border-bottom: 1px solid var(--color-border-default);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .matrix-row {
    display: grid;
    grid-template-columns: 1fr 2fr auto;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
    align-items: center;
    transition: background var(--duration-fast) var(--easing-default);
  }

  .matrix-row:last-of-type {
    border-bottom: none;
  }

  .matrix-row:hover {
    background: var(--color-bg-elevated-1);
  }

  .matrix-row.failed {
    background: color-mix(in srgb, var(--color-severity-hard) 5%, transparent);
  }

  .row-dim {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
  }

  .dim-key {
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
    min-width: 2ch;
  }

  .dim-label {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .row-score {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
  }

  .score-bar {
    flex: 1;
    height: 6px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-full);
    position: relative;
    overflow: visible;
  }

  .score-fill {
    height: 100%;
    border-radius: var(--radius-full);
    transition: width var(--duration-normal) var(--easing-default);
  }

  .threshold-mark {
    position: absolute;
    top: -2px;
    width: 2px;
    height: 10px;
    background: var(--color-text-disabled);
    transform: translateX(-50%);
  }

  .score-value {
    min-width: 4ch;
    text-align: right;
    color: var(--color-text-secondary);
    font-variant-numeric: tabular-nums;
  }

  .status-badge {
    padding: var(--spacing-0) var(--spacing-1);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-bold);
    letter-spacing: var(--letter-spacing-wide);
  }

  .status-badge.pass {
    color: var(--color-severity-normal);
    background: color-mix(in srgb, var(--color-severity-normal) 15%, transparent);
  }

  .status-badge.fail {
    color: var(--color-severity-hard);
    background: color-mix(in srgb, var(--color-severity-hard) 15%, transparent);
  }

  .matrix-footer {
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border-top: 1px solid var(--color-border-default);
    text-align: right;
  }

  .overall {
    font-weight: var(--font-weight-bold);
    font-size: var(--font-size-sm);
  }

  .overall.pass { color: var(--color-severity-normal); }
  .overall.fail { color: var(--color-severity-hard); }

  .compact .dim-label { display: none; }
  .compact .matrix-header { display: none; }
  .compact .matrix-row {
    grid-template-columns: auto 1fr auto;
    padding: var(--spacing-1) var(--spacing-2);
  }
</style>
