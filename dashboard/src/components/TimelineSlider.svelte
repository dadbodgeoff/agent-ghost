<!--
  TimelineSlider — Accessible range slider for session replay (T-X.9).

  Supports keyboard navigation (Left/Right arrows), mouse drag,
  and touch drag. Displays current position and total count.

  Ref: ADE_DESIGN_PLAN §5.5.2, tasks.md T-X.9
-->
<script lang="ts">
  interface Props {
    min?: number;
    max: number;
    value?: number;
    label?: string;
    onchange?: (value: number) => void;
  }

  let { min = 0, max, value = $bindable(0), label = 'Event', onchange }: Props = $props();

  let trackEl: HTMLDivElement | undefined = $state();
  let dragging = $state(false);

  let progress = $derived(max > min ? ((value - min) / (max - min)) * 100 : 0);

  function clamp(v: number): number {
    return Math.max(min, Math.min(max, Math.round(v)));
  }

  function updateFromPointer(clientX: number) {
    if (!trackEl) return;
    const rect = trackEl.getBoundingClientRect();
    const ratio = (clientX - rect.left) / rect.width;
    const newValue = clamp(min + ratio * (max - min));
    if (newValue !== value) {
      value = newValue;
      onchange?.(value);
    }
  }

  function onPointerDown(e: PointerEvent) {
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    updateFromPointer(e.clientX);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    updateFromPointer(e.clientX);
  }

  function onPointerUp() {
    dragging = false;
  }

  function onKeyDown(e: KeyboardEvent) {
    let newValue = value;
    switch (e.key) {
      case 'ArrowLeft':
      case 'ArrowDown':
        newValue = clamp(value - 1);
        e.preventDefault();
        break;
      case 'ArrowRight':
      case 'ArrowUp':
        newValue = clamp(value + 1);
        e.preventDefault();
        break;
      case 'Home':
        newValue = min;
        e.preventDefault();
        break;
      case 'End':
        newValue = max;
        e.preventDefault();
        break;
      case 'PageUp':
        newValue = clamp(value + Math.max(1, Math.floor((max - min) / 10)));
        e.preventDefault();
        break;
      case 'PageDown':
        newValue = clamp(value - Math.max(1, Math.floor((max - min) / 10)));
        e.preventDefault();
        break;
    }
    if (newValue !== value) {
      value = newValue;
      onchange?.(value);
    }
  }
</script>

<div class="timeline-slider">
  <span class="slider-label">{label} {value} / {max}</span>
  <div
    class="slider-track"
    bind:this={trackEl}
    role="slider"
    tabindex="0"
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuenow={value}
    aria-label={`${label} position`}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onkeydown={onKeyDown}
  >
    <div class="slider-fill" style="width: {progress}%"></div>
    <div
      class="slider-thumb"
      class:dragging
      style="left: {progress}%"
    ></div>
  </div>
</div>

<style>
  .timeline-slider {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
    width: 100%;
  }

  .slider-label {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .slider-track {
    position: relative;
    height: 8px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-full);
    cursor: pointer;
    touch-action: none;
    outline: none;
  }

  .slider-track:focus-visible {
    box-shadow: var(--shadow-focus-ring);
  }

  .slider-fill {
    position: absolute;
    top: 0;
    left: 0;
    height: 100%;
    background: var(--color-interactive-primary);
    border-radius: var(--radius-full);
    transition: width 50ms linear;
    pointer-events: none;
  }

  .slider-thumb {
    position: absolute;
    top: 50%;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-full);
    background: var(--color-interactive-primary);
    border: 2px solid var(--color-bg-base);
    box-shadow: var(--shadow-elevated-1);
    transform: translate(-50%, -50%);
    transition: transform var(--duration-fast) var(--easing-default);
    pointer-events: none;
  }

  .slider-thumb.dragging {
    transform: translate(-50%, -50%) scale(1.2);
  }

  @media (max-width: 640px) {
    .slider-track {
      height: 12px;
    }
    .slider-thumb {
      width: 20px;
      height: 20px;
    }
  }
</style>
