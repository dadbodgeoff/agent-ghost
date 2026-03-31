<script lang="ts">
  import type { Skill } from '@ghost/sdk';
  import CapabilityBadge from './CapabilityBadge.svelte';

  export type SkillCardAction =
    | 'install'
    | 'uninstall'
    | 'quarantine'
    | 'resolve'
    | 'reverify';

  interface Props {
    skill: Skill;
    installed: boolean;
    onAction: (skill: Skill, action: SkillCardAction) => void;
    loading?: boolean;
  }

  type ActionSpec = {
    id: SkillCardAction;
    label: string;
    tone?: 'danger' | 'secondary';
  };

  let { skill, installed, onAction, loading = false }: Props = $props();

  const STATE_LABELS: Record<Skill['state'], string> = {
    always_on: 'Always on',
    installed: 'Installed',
    available: 'Available',
    disabled: 'Disabled',
    verified: 'Verified',
    quarantined: 'Quarantined',
    verification_failed: 'Verification failed',
  };

  function isExternalSkill(skill: Skill): boolean {
    return skill.source !== 'compiled';
  }

  function resolveActions(skill: Skill, installed: boolean): ActionSpec[] {
    const actions: ActionSpec[] = [];

    if (installed) {
      if (skill.removable) {
        actions.push({ id: 'uninstall', label: 'Uninstall', tone: 'danger' });
      }
    } else if (
      skill.installable &&
      (!isExternalSkill(skill) ||
        (skill.quarantine_state === 'clear' && skill.verification_status === 'verified'))
    ) {
      actions.push({ id: 'install', label: 'Install' });
    }

    if (!isExternalSkill(skill)) {
      return actions;
    }

    actions.push({ id: 'reverify', label: 'Reverify', tone: 'secondary' });
    if (skill.quarantine_state === 'quarantined') {
      if (skill.verification_status === 'verified' && skill.quarantine_revision != null) {
        actions.push({ id: 'resolve', label: 'Resolve', tone: 'secondary' });
      }
    } else {
      actions.push({ id: 'quarantine', label: 'Quarantine', tone: 'danger' });
    }

    return actions;
  }

  const actions = $derived.by(() => resolveActions(skill, installed));
  const runtimeNote = $derived.by(() => {
    if (isExternalSkill(skill) && skill.install_state === 'installed' && !skill.runtime_visible) {
      return 'Installed in catalog only. Runtime execution remains gated off.';
    }
    if (skill.quarantine_state === 'quarantined' && skill.quarantine_reason) {
      return skill.quarantine_reason;
    }
    return null;
  });
</script>

<div class="skill-card" class:installed>
  <div class="skill-header">
    <div>
      <div class="skill-name">{skill.name}</div>
      <div class="skill-version">v{skill.version}</div>
    </div>
    <span class="state-badge state-{skill.state}">{STATE_LABELS[skill.state]}</span>
  </div>

  <p class="skill-desc">{skill.description}</p>

  <div class="skill-meta">
    <span class="skill-source">{skill.source}</span>
    <span class="skill-mode">{skill.execution_mode}</span>
    <span>verify:{skill.verification_status}</span>
    <span>install:{skill.install_state}</span>
    <span>runtime:{skill.runtime_visible ? 'visible' : 'gated'}</span>
  </div>

  {#if skill.publisher || skill.signer_publisher}
    <div class="trust-row">
      {#if skill.publisher}
        <span>Publisher: {skill.publisher}</span>
      {/if}
      {#if skill.signer_publisher}
        <span>Signer: {skill.signer_publisher}</span>
      {/if}
    </div>
  {/if}

  <div class="policy-row">
    <span class="section-label">Policy</span>
    <CapabilityBadge capability={skill.policy_capability} />
  </div>

  <div class="privilege-row">
    <span class="section-label">Privileges</span>
    {#if skill.privileges.length > 0}
      <ul class="privilege-list">
        {#each skill.privileges.slice(0, 2) as privilege}
          <li>{privilege}</li>
        {/each}
        {#if skill.privileges.length > 2}
          <li>+{skill.privileges.length - 2} more</li>
        {/if}
      </ul>
    {:else}
      <p class="no-privileges">No elevated privileges declared.</p>
    {/if}
  </div>

  {#if runtimeNote}
    <p class="runtime-note">{runtimeNote}</p>
  {/if}

  <div class="skill-footer">
    {#if actions.length === 0}
      <button class="action-btn secondary" disabled>
        {skill.state === 'always_on' ? 'Always on' : 'Unavailable'}
      </button>
    {:else}
      {#each actions as action}
        <button
          type="button"
          class="action-btn"
          class:danger={action.tone === 'danger'}
          class:secondary={action.tone === 'secondary'}
          disabled={loading}
          onclick={() => onAction(skill, action.id)}
        >
          {loading ? '...' : action.label}
        </button>
      {/each}
    {/if}
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
    margin-top: 0.25rem;
  }

  .skill-desc {
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    margin: 0;
  }

  .state-badge {
    display: inline-flex;
    align-items: center;
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    border: 1px solid transparent;
    white-space: nowrap;
  }

  .state-always_on {
    background: color-mix(in srgb, var(--color-score-high) 14%, transparent);
    color: var(--color-score-high);
    border-color: color-mix(in srgb, var(--color-score-high) 28%, transparent);
  }

  .state-installed {
    background: color-mix(in srgb, var(--color-brand-primary) 14%, transparent);
    color: var(--color-brand-primary);
    border-color: color-mix(in srgb, var(--color-brand-primary) 28%, transparent);
  }

  .state-available,
  .state-disabled {
    background: color-mix(in srgb, var(--color-text-muted) 10%, transparent);
    color: var(--color-text-secondary);
    border-color: color-mix(in srgb, var(--color-text-muted) 24%, transparent);
  }

  .state-verified {
    background: color-mix(in srgb, var(--color-score-high) 10%, transparent);
    color: var(--color-score-high);
    border-color: color-mix(in srgb, var(--color-score-high) 24%, transparent);
  }

  .state-quarantined,
  .state-verification_failed {
    background: color-mix(in srgb, var(--color-severity-hard) 12%, transparent);
    color: var(--color-severity-hard);
    border-color: color-mix(in srgb, var(--color-severity-hard) 28%, transparent);
  }

  .skill-meta,
  .trust-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-2);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .skill-meta {
    text-transform: lowercase;
  }

  .policy-row,
  .privilege-row {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .section-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .privilege-list {
    margin: 0;
    padding-left: 1rem;
    display: grid;
    gap: var(--spacing-1);
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
    line-height: 1.5;
  }

  .no-privileges,
  .runtime-note {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    line-height: 1.5;
  }

  .runtime-note {
    padding: var(--spacing-2) var(--spacing-3);
    border-radius: var(--radius-sm);
    background: var(--color-bg-elevated-2);
  }

  .skill-footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: var(--spacing-2);
    margin-top: auto;
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

  .action-btn.secondary {
    background: transparent;
    color: var(--color-text-secondary);
    border-color: var(--color-border-default);
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
