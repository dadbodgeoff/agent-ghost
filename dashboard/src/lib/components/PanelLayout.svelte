<script lang="ts">
  import { Splitpanes, Pane } from 'svelte-splitpanes';
  import type { Snippet } from 'svelte';

  interface Props {
    sidebar: Snippet;
    main: Snippet;
    bottom?: Snippet;
    sidebarFooter?: Snippet;
  }

  let { sidebar, main, bottom, sidebarFooter }: Props = $props();

  let bottomCollapsed = $state(false);

  function toggleBottom() {
    bottomCollapsed = !bottomCollapsed;
  }
</script>

<div class="panel-layout">
  <Splitpanes horizontal={false}>
    <Pane minSize={10} size={15} maxSize={25}>
      <div class="sidebar-pane">
        <div class="sidebar-content">
          {@render sidebar()}
        </div>
        {#if sidebarFooter}
          <div class="sidebar-footer">
            {@render sidebarFooter()}
          </div>
        {/if}
      </div>
    </Pane>

    <Pane minSize={40}>
      <div class="center-pane">
        {#if bottom && !bottomCollapsed}
          <Splitpanes horizontal={true}>
            <Pane minSize={30}>
              <div class="main-content">
                {@render main()}
              </div>
            </Pane>
            <Pane minSize={10} size={25} maxSize={40}>
              <div class="bottom-pane">
                <div class="bottom-header">
                  <span class="bottom-title">Terminal</span>
                  <button class="bottom-toggle" onclick={toggleBottom} aria-label="Collapse panel">&times;</button>
                </div>
                <div class="bottom-content">
                  {@render bottom()}
                </div>
              </div>
            </Pane>
          </Splitpanes>
        {:else}
          <div class="main-content">
            {@render main()}
          </div>
          {#if bottom && bottomCollapsed}
            <button class="bottom-expand" onclick={toggleBottom}>Show Panel</button>
          {/if}
        {/if}
      </div>
    </Pane>
  </Splitpanes>
</div>

<style>
  .panel-layout {
    height: 100vh;
    width: 100%;
  }

  .panel-layout :global(.splitpanes) {
    height: 100%;
    background: var(--color-bg-base);
  }

  .panel-layout :global(.splitpanes__pane) {
    background: var(--color-bg-base);
  }

  .panel-layout :global(.splitpanes__splitter) {
    background: var(--color-border-default);
    position: relative;
  }

  /* Vertical splitter (between sidebar and center) */
  .panel-layout > :global(.splitpanes > .splitpanes__splitter) {
    width: 3px;
    min-width: 3px;
  }

  .panel-layout :global(.splitpanes__splitter:hover) {
    background: var(--color-brand-primary);
  }

  /* Horizontal splitter (between main and bottom) */
  .center-pane :global(.splitpanes__splitter) {
    height: 3px;
    min-height: 3px;
  }

  .sidebar-pane {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-elevated-2);
    overflow: hidden;
  }

  .sidebar-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--spacing-4);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .sidebar-footer {
    padding: var(--spacing-3) var(--spacing-4);
    border-top: 1px solid var(--color-border-subtle);
  }

  .center-pane {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-bg-base);
  }

  .center-pane > :global(.splitpanes) {
    flex: 1;
  }

  .main-content {
    flex: 1;
    overflow-y: auto;
    padding: var(--layout-content-padding);
    background: var(--color-bg-base);
  }

  .bottom-pane {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-elevated-2);
    overflow: hidden;
  }

  .bottom-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-1) var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated-2);
  }

  .bottom-title {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wider);
  }

  .bottom-toggle {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-size: 16px;
    cursor: pointer;
  }

  .bottom-toggle:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }

  .bottom-content {
    flex: 1;
    overflow-y: auto;
  }

  .bottom-expand {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: none;
    border-top: 1px solid var(--color-border-default);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    cursor: pointer;
    text-align: center;
    width: 100%;
  }

  .bottom-expand:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }
</style>
