<script lang="ts">
  import { goto } from '$app/navigation';
  import { tabStore, type Tab } from '$lib/stores/tabs.svelte';

  function activate(tab: Tab) {
    tabStore.activeId = tab.id;
    goto(tab.href);
  }

  function handleActivateKeydown(e: KeyboardEvent, tab: Tab) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      activate(tab);
    }
  }

  function close(e: MouseEvent, tab: Tab) {
    e.stopPropagation();
    const wasActive = tabStore.activeId === tab.id;
    tabStore.close(tab.id);
    // Navigate to the new active tab if we closed the current one
    if (wasActive && tabStore.active) {
      goto(tabStore.active.href);
    }
  }
</script>

<div class="tab-bar" role="tablist">
  {#each tabStore.tabs as tab (tab.id)}
    <div
      class="tab"
      class:active={tabStore.activeId === tab.id}
      role="tab"
      tabindex="0"
      aria-selected={tabStore.activeId === tab.id}
      onclick={() => activate(tab)}
      onkeydown={(e) => handleActivateKeydown(e, tab)}
    >
      <span class="tab-label">{tab.label}</span>
      {#if tab.closable}
        <button
          type="button"
          class="tab-close"
          aria-label={`Close ${tab.label}`}
          onclick={(e) => close(e, tab)}
        >&times;</button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: stretch;
    background: var(--color-bg-elevated-2);
    border-bottom: 1px solid var(--color-border-default);
    min-height: 36px;
    overflow-x: auto;
    scrollbar-width: none;
  }

  .tab-bar::-webkit-scrollbar {
    display: none;
  }

  .tab {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-1) var(--spacing-3);
    background: transparent;
    border: none;
    border-right: 1px solid var(--color-border-subtle);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--duration-fast) var(--easing-default),
                color var(--duration-fast) var(--easing-default);
  }

  .tab:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }

  .tab.active {
    background: var(--color-bg-base);
    color: var(--color-text-primary);
    border-bottom: 2px solid var(--color-brand-primary);
  }

  .tab-label {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tab-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-text-disabled);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
  }

  .tab-close:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }
</style>
