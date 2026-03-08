<script lang="ts">
  import type { Skill } from '@ghost/sdk';
  import CapabilityBadge from './CapabilityBadge.svelte';

  interface Props {
    skill: Skill;
    installed: boolean;
    onAction: (skill: Skill, action: 'install' | 'uninstall') => void;
    loading?: boolean;
  }

  let { skill, installed, onAction, loading = false }: Props = $props();
</script>

<div class="skill-card" class:installed>
  <div class="skill-header">
    <div class="skill-name">{skill.name}</div>
    <span class="skill-version">v{skill.version}</span>
  </div>
  <p class="skill-desc">{skill.description}</p>
  <div class="skill-caps">
    {#each skill.capabilities as cap}
      <CapabilityBadge capability={cap} />
    {/each}
  </div>
  <div class="skill-footer">
    <span class="skill-source">{skill.source}</span>
    <button
      class="action-btn"
      class:danger={installed}
      disabled={loading}
      onclick={() => onAction(skill, installed ? 'uninstall' : 'install')}
    >
      {#if loading}
        ...
      {:else if installed}
        Uninstall
      {:else}
        Install
      {/if}
    </button>
  </div>
</div>

<style>
  .skill-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    transition: border-color var(--duration-fast) var(--easing-default);
  }

  .skill-card:hover {
    border-color: var(--color-border-emphasis);
  }

  .skill-card.installed {
    border-left: 3px solid var(--color-score-high);
  }

  .skill-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-2);
  }

  .skill-name {
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .skill-version {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-variant-numeric: tabular-nums;
  }

  .skill-desc {
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    margin: 0;
  }

  .skill-caps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-1);
  }

  .skill-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: auto;
  }

  .skill-source {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: capitalize;
  }

  .action-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: 1px solid transparent;
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
    transition: background var(--duration-fast) var(--easing-default);
  }

  .action-btn:hover:not(:disabled) {
    opacity: 0.9;
  }

  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .action-btn.danger {
    background: transparent;
    color: var(--color-severity-hard);
    border-color: var(--color-severity-hard);
  }

  .action-btn.danger:hover:not(:disabled) {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }
</style>
