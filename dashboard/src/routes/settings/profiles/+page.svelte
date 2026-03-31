<script lang="ts">
  /**
   * Profile editor — convergence profile CRUD with weight sliders.
   * 4 presets + custom profiles.
   *
   * Ref: T-3.10.1
   */
  import { onMount } from 'svelte';
  import type { Profile } from '@ghost/sdk';
  import { getGhostClient } from '$lib/ghost-client';
  import WeightSlider from '../../../components/WeightSlider.svelte';

  let profiles: Profile[] = $state([]);
  let selectedProfile: Profile | null = $state(null);
  let editWeights: number[] = $state([0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125, 0.125]);
  let editThresholds: number[] = $state([0.3, 0.5, 0.7, 0.85]);
  let newProfileName = $state('');
  let saving = $state(false);
  let error: string | null = $state(null);
  let success: string | null = $state(null);

  onMount(() => {
    void loadProfiles();
  });

  async function loadProfiles() {
    error = null;
    try {
      const client = await getGhostClient();
      const res = await client.profiles.list();
      profiles = res.profiles ?? [];
      if (selectedProfile) {
        const refreshedProfile = profiles.find((profile) => profile.name === selectedProfile?.name) ?? null;
        if (refreshedProfile) {
          selectProfile(refreshedProfile);
        } else if (profiles.length > 0) {
          selectProfile(profiles[0]);
        } else {
          selectedProfile = null;
        }
      } else if (profiles.length > 0) {
        selectProfile(profiles[0]);
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load profiles';
    }
  }

  function selectProfile(p: Profile) {
    selectedProfile = p;
    editWeights = [...p.weights];
    editThresholds = [...p.thresholds];
  }

  async function saveProfile() {
    if (!selectedProfile || selectedProfile.is_preset) return;
    saving = true;
    error = null;
    success = null;
    try {
      const client = await getGhostClient();
      await client.profiles.update(selectedProfile.name, {
        weights: editWeights,
        thresholds: editThresholds,
      });
      success = `Profile "${selectedProfile.name}" saved.`;
      await loadProfiles();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to save profile';
    } finally {
      saving = false;
    }
  }

  async function createProfile() {
    const profileName = newProfileName.trim();
    if (!profileName) return;
    saving = true;
    error = null;
    try {
      const client = await getGhostClient();
      await client.profiles.create({
        name: profileName,
        weights: editWeights,
        thresholds: editThresholds,
      });
      newProfileName = '';
      success = 'Profile created.';
      await loadProfiles();
      const createdProfile = profiles.find((profile) => profile.name === profileName);
      if (createdProfile) {
        selectProfile(createdProfile);
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to create profile';
    } finally {
      saving = false;
    }
  }

  async function deleteProfile() {
    if (!selectedProfile || selectedProfile.is_preset) return;
    if (!confirm(`Delete profile "${selectedProfile.name}"?`)) return;
    saving = true;
    error = null;
    try {
      const client = await getGhostClient();
      await client.profiles.delete(selectedProfile.name);
      selectedProfile = null;
      success = 'Profile deleted.';
      await loadProfiles();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to delete profile';
    } finally {
      saving = false;
    }
  }

  const THRESHOLD_LABELS = ['L1 (Soft)', 'L2 (Active)', 'L3 (Hard)', 'L4 (External)'];
</script>

<svelte:head>
  <title>Profiles | Settings | ADE</title>
</svelte:head>

<div class="profiles-page">
  <header class="page-header">
    <h1>Convergence Profiles</h1>
    <p class="subtitle">Configure signal weights and intervention thresholds</p>
  </header>

  {#if error}
    <p class="msg error-msg">{error}</p>
  {/if}
  {#if success}
    <p class="msg success-msg">{success}</p>
  {/if}

  <div class="layout">
    <aside class="profile-list">
      <h2>Profiles</h2>
      {#each profiles as p}
        <button
          class="profile-btn"
          class:active={selectedProfile?.name === p.name}
          onclick={() => selectProfile(p)}
        >
          <span class="profile-name">{p.name}</span>
          {#if p.is_preset}
            <span class="preset-badge">Preset</span>
          {/if}
        </button>
      {/each}

      <div class="new-profile-form">
        <input
          type="text"
          placeholder="New profile name"
          bind:value={newProfileName}
          class="input-field"
        />
        <button class="action-btn" onclick={createProfile} disabled={saving || !newProfileName.trim()}>
          Create
        </button>
      </div>
    </aside>

    <main class="editor">
      {#if selectedProfile}
        <h2>{selectedProfile.name}</h2>
        <p class="description">{selectedProfile.description}</p>

        <WeightSlider
          weights={editWeights}
          onchange={(w) => editWeights = w}
          disabled={selectedProfile.is_preset}
        />

        <div class="thresholds-section">
          <h3>Intervention Thresholds</h3>
          {#each editThresholds as threshold, i}
            <div class="threshold-row">
              <label for="threshold-{i}">{THRESHOLD_LABELS[i]}</label>
              <input
                id="threshold-{i}"
                type="range"
                min="0"
                max="1"
                step="0.01"
                bind:value={editThresholds[i]}
                disabled={selectedProfile.is_preset}
                class="slider"
              />
              <span class="threshold-value mono">{threshold.toFixed(2)}</span>
            </div>
          {/each}
        </div>

        {#if !selectedProfile.is_preset}
          <div class="actions">
            <button class="action-btn primary" onclick={saveProfile} disabled={saving}>
              {saving ? 'Saving…' : 'Save'}
            </button>
            <button class="action-btn danger" onclick={deleteProfile} disabled={saving}>
              Delete
            </button>
          </div>
        {:else}
          <p class="readonly-hint">Preset profiles are read-only. Create a custom profile to modify.</p>
        {/if}
      {:else}
        <p class="placeholder">Select a profile to view and edit its configuration.</p>
      {/if}
    </main>
  </div>
</div>

<style>
  .profiles-page {
    padding: var(--spacing-6);
    max-width: 1000px;
  }

  .page-header { margin-bottom: var(--spacing-6); }
  .page-header h1 { font-size: var(--font-size-2xl); font-weight: 700; color: var(--color-text-primary); }
  .subtitle { color: var(--color-text-muted); font-size: var(--font-size-sm); margin-top: var(--spacing-1); }

  .layout { display: grid; grid-template-columns: 220px 1fr; gap: var(--spacing-4); }

  .profile-list {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
  }

  .profile-list h2 {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-2);
    text-transform: uppercase;
  }

  .profile-btn {
    width: 100%;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    cursor: pointer;
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    margin-bottom: var(--spacing-1);
  }

  .profile-btn:hover { background: var(--color-bg-elevated-2); }
  .profile-btn.active { background: var(--color-bg-elevated-2); border-color: var(--color-interactive-primary); }

  .preset-badge {
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
  }

  .new-profile-form {
    margin-top: var(--spacing-3);
    display: flex;
    gap: var(--spacing-1);
  }

  .input-field {
    flex: 1;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-2);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .editor h2 { font-size: var(--font-size-lg); font-weight: 600; color: var(--color-text-primary); }
  .description { color: var(--color-text-muted); font-size: var(--font-size-sm); margin-bottom: var(--spacing-4); }

  .thresholds-section { margin-top: var(--spacing-4); }
  .thresholds-section h3 { font-size: var(--font-size-sm); font-weight: 600; color: var(--color-text-primary); margin-bottom: var(--spacing-2); }

  .threshold-row {
    display: grid;
    grid-template-columns: 140px 1fr 60px;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-1) 0;
  }

  .threshold-row label { font-size: var(--font-size-sm); color: var(--color-text-secondary); }
  .slider { accent-color: var(--color-interactive-primary); }
  .threshold-value { font-family: var(--font-family-mono); font-size: var(--font-size-sm); text-align: right; font-variant-numeric: tabular-nums; }

  .actions { margin-top: var(--spacing-4); }

  .action-btn {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-3);
    cursor: pointer;
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .action-btn.primary { background: var(--color-interactive-primary); color: var(--color-text-inverse); border-color: var(--color-interactive-primary); }
  .action-btn.danger { background: transparent; color: var(--color-severity-hard); border-color: var(--color-severity-hard); }
  .action-btn.danger:hover { background: var(--color-severity-hard); color: var(--color-text-inverse); }
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .readonly-hint { color: var(--color-text-muted); font-size: var(--font-size-xs); margin-top: var(--spacing-3); }
  .placeholder { color: var(--color-text-muted); font-size: var(--font-size-sm); padding: var(--spacing-8); text-align: center; }
  .msg { padding: var(--spacing-2) var(--spacing-3); border-radius: var(--radius-sm); font-size: var(--font-size-sm); margin-bottom: var(--spacing-3); }
  .error-msg { background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-hard); color: var(--color-severity-hard); }
  .success-msg { background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-normal); color: var(--color-severity-normal); }
  .mono { font-family: var(--font-family-mono); }
</style>
