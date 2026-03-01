<!--
  GateCheckBar — 6 gate states visualization (T-X.7).

  Displays the current state of all 6 safety gates:
  Circuit Breaker, Depth, Damage, Cap, Convergence, Hash.

  Each gate shows pass/fail/warning/unknown status with color-coded indicators.

  Ref: ADE_DESIGN_PLAN §5.2.7, tasks.md T-X.7
-->
<script lang="ts">
  interface GateState {
    name: string;
    status: 'pass' | 'fail' | 'warning' | 'unknown';
    detail?: string;
  }

  interface Props {
    gates?: GateState[];
    compact?: boolean;
  }

  let { gates = defaultGates(), compact = false }: Props = $props();

  function defaultGates(): GateState[] {
    return [
      { name: 'CB', status: 'unknown', detail: 'Circuit Breaker' },
      { name: 'Depth', status: 'unknown', detail: 'Recursion Depth' },
      { name: 'Damage', status: 'unknown', detail: 'Damage Assessment' },
      { name: 'Cap', status: 'unknown', detail: 'Spending Cap' },
      { name: 'Conv', status: 'unknown', detail: 'Convergence' },
      { name: 'Hash', status: 'unknown', detail: 'Hash Chain' },
    ];
  }

  const statusColors: Record<string, string> = {
    pass: 'var(--color-severity-normal)',
    fail: 'var(--color-severity-hard)',
    warning: 'var(--color-severity-soft)',
    unknown: 'var(--color-text-disabled)',
  };

  const statusIcons: Record<string, string> = {
    pass: '\u2713',
    fail: '\u2717',
    warning: '\u26A0',
    unknown: '\u2014',
  };

  let passCount = $derived(gates.filter(g => g.status === 'pass').length);
  let failCount = $derived(gates.filter(g => g.status === 'fail').length);
</script>

<div
  class="gate-check-bar"
  class:compact
  role="group"
  aria-label="Safety gate status: {passCount} passed, {failCount} failed"
>
  {#each gates as gate}
    <div
      class="gate"
      title="{gate.detail ?? gate.name}: {gate.status}"
      style="--gate-color: {statusColors[gate.status]}"
    >
      <span class="gate-icon" aria-hidden="true">{statusIcons[gate.status]}</span>
      {#if !compact}
        <span class="gate-label">{gate.name}</span>
      {/if}
    </div>
  {/each}
</div>

<style>
  .gate-check-bar {
    display: flex;
    gap: var(--spacing-2);
    align-items: center;
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
  }

  .gate-check-bar.compact {
    gap: var(--spacing-1);
    padding: var(--spacing-1);
  }

  .gate {
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
    padding: var(--spacing-0) var(--spacing-1);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--gate-color) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--gate-color) 30%, transparent);
    cursor: default;
  }

  .gate-icon {
    font-size: var(--font-size-sm);
    color: var(--gate-color);
    font-weight: var(--font-weight-bold);
    line-height: 1;
  }

  .gate-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    font-family: var(--font-family-mono);
    letter-spacing: var(--letter-spacing-tight);
  }

  .compact .gate-label {
    display: none;
  }
</style>
