<script lang="ts">
  import { onMount } from 'svelte';

  /**
   * ConfirmDialog — destructive action confirmation modal.
   *
   * Uses Svelte 5 runes. Parent passes callback props `onconfirm` and `oncancel`.
   * Ref: T-X.16, DESIGN_SYSTEM §8.4
   */

  let {
    title = 'Confirm',
    message = 'Are you sure?',
    confirmLabel = 'Confirm',
    danger = false,
    loading = false,
    onconfirm,
    oncancel,
  }: {
    title?: string;
    message?: string;
    confirmLabel?: string;
    danger?: boolean;
    loading?: boolean;
    onconfirm?: () => void;
    oncancel?: () => void;
  } = $props();

  function confirm() {
    onconfirm?.();
  }

  function cancel() {
    oncancel?.();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') cancel();
  }

  let dialogEl: HTMLDivElement | null = null;

  onMount(() => {
    dialogEl?.focus();
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="overlay" onclick={cancel} role="presentation">
  <div
    bind:this={dialogEl}
    class="dialog"
    role="alertdialog"
    tabindex="-1"
    aria-modal="true"
    aria-labelledby="confirm-title"
    aria-describedby="confirm-message"
    onclick={(e) => e.stopPropagation()}
  >
    <h2 id="confirm-title">{title}</h2>
    <p id="confirm-message">{message}</p>
    <div class="actions">
      <button class="cancel-btn" onclick={cancel} disabled={loading}>Cancel</button>
      <button
        class="confirm-btn"
        class:danger
        onclick={confirm}
        disabled={loading}
      >
        {#if loading}
          Working…
        {:else}
          {confirmLabel}
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--color-bg-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .dialog {
    background: var(--color-bg-elevated-3);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-lg);
    padding: var(--spacing-6);
    max-width: 400px;
    width: 90%;
    box-shadow: var(--shadow-elevated-3);
  }

  h2 {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
    margin-bottom: var(--spacing-2);
  }

  p {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    line-height: var(--line-height-normal);
    margin-bottom: var(--spacing-6);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--spacing-2);
  }

  .cancel-btn {
    padding: var(--spacing-2) var(--spacing-4);
    background: transparent;
    border: 1px solid var(--color-interactive-secondary-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--duration-fast) var(--easing-default);
  }

  .cancel-btn:hover:not(:disabled) {
    background: var(--color-interactive-secondary-hover);
  }

  .cancel-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .confirm-btn {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
    transition: background var(--duration-fast) var(--easing-default);
  }

  .confirm-btn:hover:not(:disabled) {
    background: var(--color-interactive-primary-hover);
  }

  .confirm-btn.danger {
    background: var(--color-interactive-danger);
  }

  .confirm-btn.danger:hover:not(:disabled) {
    background: var(--color-interactive-danger-hover);
  }

  .confirm-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .confirm-btn:focus-visible,
  .cancel-btn:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus-ring);
  }
</style>
