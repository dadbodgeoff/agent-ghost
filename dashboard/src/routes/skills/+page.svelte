<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import type { Skill } from '@ghost/sdk';
  import SkillCard from '../../components/SkillCard.svelte';
  import CapabilityBadge from '../../components/CapabilityBadge.svelte';

  let installed = $state<Skill[]>([]);
  let available = $state<Skill[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let actionLoading = $state<string | null>(null);
  let activeTab = $state<'installed' | 'available'>('installed');
  let confirmSkill = $state<Skill | null>(null);
  let quarantineSkill = $state<Skill | null>(null);
  let quarantineReason = $state('');

  type SkillAction = 'install' | 'uninstall' | 'quarantine' | 'resolve' | 'reverify';

  onMount(() => {
    loadSkills();

    // T-5.9.1: Wire SkillChange WS event to refresh skills list.
    const unsub = wsStore.on('SkillChange', () => { loadSkills(); });
    const unsubResync = wsStore.onResync(() => { loadSkills(); });
    return () => {
      unsub();
      unsubResync();
    };
  });

  async function loadSkills() {
    loading = true;
    error = null;
    try {
      const client = await getGhostClient();
      const data = await client.skills.list();
      installed = data.installed ?? [];
      available = data.available ?? [];
    } catch (e: unknown) {
      // T-5.9.2: Show error instead of swallowing.
      error = e instanceof Error ? e.message : 'Failed to load skills';
    } finally {
      loading = false;
    }
  }

  async function handleAction(skill: Skill, action: SkillAction) {
    if (action === 'install') {
      if (!skill.installable) return;
      confirmSkill = skill;
      return;
    }
    if (action === 'uninstall' && !skill.removable) return;
    if (action === 'quarantine') {
      quarantineSkill = skill;
      quarantineReason = skill.quarantine_reason ?? '';
      return;
    }
    await doAction(skill, action);
  }

  async function confirmInstall() {
    if (!confirmSkill) return;
    await doAction(confirmSkill, 'install');
    confirmSkill = null;
  }

  async function confirmQuarantine() {
    if (!quarantineSkill || !quarantineReason.trim()) return;
    await doAction(quarantineSkill, 'quarantine', quarantineReason.trim());
    if (!error) {
      quarantineSkill = null;
      quarantineReason = '';
    }
  }

  function closeQuarantineDialog() {
    quarantineSkill = null;
    quarantineReason = '';
  }

  async function doAction(skill: Skill, action: SkillAction, reason?: string) {
    actionLoading = skill.id;
    try {
      const client = await getGhostClient();
      switch (action) {
        case 'install':
          await client.skills.install(skill.id);
          break;
        case 'uninstall':
          await client.skills.uninstall(skill.id);
          break;
        case 'quarantine':
          await client.skills.quarantine(skill.id, { reason: reason ?? '' });
          break;
        case 'resolve':
          if (skill.quarantine_revision == null) {
            throw new Error('Quarantine revision is required to resolve a skill quarantine');
          }
          await client.skills.resolveQuarantine(skill.id, {
            expected_quarantine_revision: skill.quarantine_revision,
          });
          break;
        case 'reverify':
          await client.skills.reverify(skill.id);
          break;
      }
      await loadSkills();
    } catch (e: unknown) {
      // T-5.9.2: Show error to user.
      error = e instanceof Error ? e.message : `Failed to ${action} skill`;
    } finally {
      actionLoading = null;
    }
  }
</script>

<div class="page">
  <header class="page-header">
    <h1>Skills</h1>
    <p class="subtitle">
      Manage compiled and external skills through the gateway-owned catalog, verification, and
      quarantine state
    </p>
  </header>

  <div class="tabs">
    <button
      class="tab"
      class:active={activeTab === 'installed'}
      onclick={() => (activeTab = 'installed')}
    >
      Installed ({installed.length})
    </button>
    <button
      class="tab"
      class:active={activeTab === 'available'}
      onclick={() => (activeTab = 'available')}
    >
      Available ({available.length})
    </button>
  </div>

  {#if error}
    <div class="error-banner">
      <p>{error}</p>
      <button onclick={() => { error = null; loadSkills(); }}>Retry</button>
    </div>
  {/if}

  {#if loading}
    <p class="loading">Loading skills...</p>
  {:else if activeTab === 'installed'}
    {#if installed.length === 0}
      <div class="empty-state">
        <p>No skills installed yet.</p>
        <button class="action-btn" onclick={() => (activeTab = 'available')}>
          Browse Available Skills
        </button>
      </div>
    {:else}
      <div class="skill-grid">
        {#each installed as skill (skill.id)}
          <SkillCard
            {skill}
            installed={true}
            onAction={handleAction}
            loading={actionLoading === skill.id}
          />
        {/each}
      </div>
    {/if}
  {:else}
    {#if available.length === 0}
      <div class="empty-state">
        <p>All available skills are already installed.</p>
      </div>
    {:else}
      <div class="skill-grid">
        {#each available as skill (skill.id)}
          <SkillCard
            {skill}
            installed={false}
            onAction={handleAction}
            loading={actionLoading === skill.id}
          />
        {/each}
      </div>
    {/if}
  {/if}
</div>

{#if confirmSkill}
  <div class="confirm-overlay" onclick={() => (confirmSkill = null)} role="presentation">
    <div
      class="confirm-dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Review privileges"
    >
      <h2>Review Install Access</h2>
      <p>
        Installing <strong>{confirmSkill.name}</strong>
        {#if confirmSkill.source === 'compiled'}
          exposes the skill to eligible runtimes and grants the following declared privileges:
        {:else}
          records the external artifact as installed in the catalog. Runtime execution is still
          subject to verification, quarantine, runtime support, and the current external WASM
          contract. Declared privileges are still shown for review:
        {/if}
      </p>
      {#if confirmSkill.privileges.length > 0}
        <ul class="privilege-list">
          {#each confirmSkill.privileges as privilege}
            <li>{privilege}</li>
          {/each}
        </ul>
      {:else}
        <p class="no-caps">No elevated privileges declared.</p>
      {/if}
      <div class="policy-review">
        <span class="policy-label">Runtime policy capability</span>
        <CapabilityBadge capability={confirmSkill.policy_capability} size="md" />
      </div>
      <div class="confirm-actions">
        <button class="cancel-btn" onclick={() => (confirmSkill = null)}>Cancel</button>
        <button class="approve-btn" onclick={confirmInstall} disabled={actionLoading === confirmSkill.id}>
          {actionLoading === confirmSkill.id ? 'Installing...' : 'Approve & Install'}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if quarantineSkill}
  <div class="confirm-overlay" onclick={closeQuarantineDialog} role="presentation">
    <div
      class="confirm-dialog"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Quarantine skill"
    >
      <h2>Quarantine External Skill</h2>
      <p>
        Quarantining <strong>{quarantineSkill.name}</strong> blocks installation and execution until
        an operator explicitly resolves the quarantine.
      </p>
      <label class="dialog-label" for="quarantine-reason">Reason</label>
      <textarea
        id="quarantine-reason"
        bind:value={quarantineReason}
        rows="4"
        placeholder="Explain why this artifact is being quarantined"
      ></textarea>
      <div class="confirm-actions">
        <button class="cancel-btn" onclick={closeQuarantineDialog}>Cancel</button>
        <button
          class="approve-btn"
          onclick={confirmQuarantine}
          disabled={actionLoading === quarantineSkill.id || !quarantineReason.trim()}
        >
          {actionLoading === quarantineSkill.id ? 'Quarantining...' : 'Confirm Quarantine'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .confirm-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-bg-overlay);
    z-index: 1000;
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .confirm-dialog {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-lg);
    padding: var(--spacing-6);
    max-width: 420px;
    width: 90%;
    box-shadow: var(--shadow-elevated-3);
  }

  .confirm-dialog h2 {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
    margin: 0 0 var(--spacing-2);
  }

  .confirm-dialog p {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    margin: 0 0 var(--spacing-4);
  }

  .privilege-list {
    margin: 0 0 var(--spacing-4);
    padding-left: 1.25rem;
    display: grid;
    gap: var(--spacing-2);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  .policy-review {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-sm);
    margin-bottom: var(--spacing-4);
  }

  .policy-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .dialog-label {
    display: block;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: var(--spacing-2);
  }

  textarea {
    width: 100%;
    min-height: 7rem;
    resize: vertical;
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
    padding: var(--spacing-3);
    font: inherit;
    margin-bottom: var(--spacing-4);
  }

  .no-caps {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    margin: 0 0 var(--spacing-4);
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--spacing-2);
  }

  .cancel-btn {
    background: transparent;
    color: var(--color-text-muted);
    border: 1px solid var(--color-border-default);
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .approve-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
    font-weight: var(--font-weight-medium);
  }

  .approve-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-6);
  }

  .page-header h1 {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
    margin: 0;
  }

  .subtitle {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin: var(--spacing-1) 0 0;
  }

  .tabs {
    display: flex;
    gap: var(--spacing-1);
    border-bottom: 1px solid var(--color-border-default);
    padding-bottom: 0;
  }

  .tab {
    background: none;
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-muted);
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color var(--duration-fast) var(--easing-default);
  }

  .tab:hover {
    color: var(--color-text-primary);
  }

  .tab.active {
    color: var(--color-brand-primary);
    border-bottom-color: var(--color-brand-primary);
  }

  .skill-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--spacing-4);
  }

  .loading {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .empty-state {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .action-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
    margin-top: var(--spacing-3);
  }
</style>
