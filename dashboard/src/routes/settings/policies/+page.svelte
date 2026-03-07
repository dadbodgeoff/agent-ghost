<script lang="ts">
  /**
   * Safety policies view — read-only for safety-critical, editable for non-critical.
   *
   * Ref: T-3.11.1
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';

  interface SafetyStatus {
    kill_all_active: boolean;
    paused_agents: string[];
    quarantined_agents: string[];
  }

  let safetyStatus: SafetyStatus = $state({ kill_all_active: false, paused_agents: [], quarantined_agents: [] });
  let spendingCap = $state(10.0);
  let recursionDepth = $state(10);
  let saving = $state(false);
  let error: string | null = $state(null);

  onMount(() => {
    loadPolicies();
  });

  async function loadPolicies() {
    try {
      const client = await getGhostClient();
      const status = await client.safety.status();
      safetyStatus = {
        kill_all_active: Boolean(status.platform_killed),
        paused_agents: Object.entries(status.per_agent ?? {})
          .filter(([, value]) => value.level?.toLowerCase() === 'pause')
          .map(([agentId]) => agentId),
        quarantined_agents: Object.entries(status.per_agent ?? {})
          .filter(([, value]) => value.level?.toLowerCase() === 'quarantine')
          .map(([agentId]) => agentId),
      };
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load safety status';
    }
  }

  let saveSuccess: string | null = $state(null);

  async function saveLimits() {
    error = null;
    saveSuccess = null;
    error = 'Gateway settings limits are not exposed by the runtime API yet.';
  }

  const SAFETY_POLICIES = [
    { name: 'Kill Switch', description: 'Emergency stop all agent operations', critical: true },
    { name: 'Credential Exfiltration Detection', description: 'Auto-kill on credential pattern detection', critical: true },
    { name: 'Simulation Boundary Enforcement', description: 'Prevent roleplay that bypasses safety', critical: true },
    { name: 'Hash Chain Verification', description: 'Tamper-evident event logging', critical: true },
    { name: 'Output Inspection', description: 'Scan all LLM responses before delivery', critical: true },
  ];
</script>

<svelte:head>
  <title>Policies | Settings | ADE</title>
</svelte:head>

<div class="policies-page">
  <header class="page-header">
    <h1>Safety Policies</h1>
    <p class="subtitle">View active safety policies and configure non-critical settings</p>
  </header>

  {#if error}
    <p class="error-msg">{error}</p>
  {/if}

  <section class="status-section">
    <h2>System Status</h2>
    <div class="status-grid">
      <div class="status-card" class:danger={safetyStatus.kill_all_active}>
        <span class="status-label">Kill Switch</span>
        <span class="status-value">{safetyStatus.kill_all_active ? 'ACTIVE' : 'Clear'}</span>
      </div>
      <div class="status-card">
        <span class="status-label">Paused Agents</span>
        <span class="status-value mono">{safetyStatus.paused_agents?.length ?? 0}</span>
      </div>
      <div class="status-card">
        <span class="status-label">Quarantined Agents</span>
        <span class="status-value mono">{safetyStatus.quarantined_agents?.length ?? 0}</span>
      </div>
    </div>
  </section>

  <section class="policies-section">
    <h2>Safety-Critical Policies</h2>
    <p class="section-hint">These policies cannot be modified from the dashboard. Use the CLI to change safety-critical settings.</p>
    <div class="policy-list">
      {#each SAFETY_POLICIES as policy}
        <div class="policy-card">
          <div class="policy-header">
            <span class="policy-name">{policy.name}</span>
            <span class="policy-badge enabled">Enabled</span>
          </div>
          <p class="policy-desc">{policy.description}</p>
        </div>
      {/each}
    </div>
  </section>

  <section class="editable-section">
    <h2>Configurable Limits</h2>
    <div class="config-row">
      <label for="spending-cap">Daily Spending Cap ($)</label>
      <input id="spending-cap" type="number" bind:value={spendingCap} step="0.5" min="0" class="input-field mono" />
    </div>
    <div class="config-row">
      <label for="recursion-depth">Max Recursion Depth</label>
      <input id="recursion-depth" type="number" bind:value={recursionDepth} step="1" min="1" max="50" class="input-field mono" />
    </div>
    <div class="save-row">
      <button class="save-btn" onclick={saveLimits} disabled={saving}>
        {saving ? 'Saving…' : 'Save Limits'}
      </button>
      {#if saveSuccess}<span class="save-success">{saveSuccess}</span>{/if}
    </div>
    <p class="section-hint">Changes take effect on next agent run.</p>
  </section>
</div>

<style>
  .policies-page { padding: var(--spacing-6); max-width: 900px; }
  .page-header { margin-bottom: var(--spacing-6); }
  .page-header h1 { font-size: var(--font-size-2xl); font-weight: 700; color: var(--color-text-primary); }
  .subtitle { color: var(--color-text-muted); font-size: var(--font-size-sm); margin-top: var(--spacing-1); }

  section { margin-bottom: var(--spacing-6); }
  section h2 { font-size: var(--font-size-lg); font-weight: 600; color: var(--color-text-primary); margin-bottom: var(--spacing-3); }

  .status-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--spacing-3); }
  .status-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
    text-align: center;
  }
  .status-card.danger { border-color: var(--color-severity-hard); }
  .status-label { display: block; font-size: var(--font-size-xs); color: var(--color-text-muted); margin-bottom: var(--spacing-1); }
  .status-value { display: block; font-size: var(--font-size-lg); font-weight: 600; color: var(--color-text-primary); }
  .status-card.danger .status-value { color: var(--color-severity-hard); }

  .section-hint { color: var(--color-text-muted); font-size: var(--font-size-xs); margin-top: var(--spacing-2); }

  .policy-list { display: flex; flex-direction: column; gap: var(--spacing-2); }
  .policy-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-3);
  }
  .policy-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: var(--spacing-1); }
  .policy-name { font-weight: 600; font-size: var(--font-size-sm); color: var(--color-text-primary); }
  .policy-badge { font-size: var(--font-size-xs); padding: 1px 8px; border-radius: var(--radius-sm); }
  .policy-badge.enabled { background: var(--color-severity-normal); color: var(--color-text-inverse); }
  .policy-desc { font-size: var(--font-size-sm); color: var(--color-text-muted); }

  .editable-section { }
  .config-row {
    display: grid;
    grid-template-columns: 200px 200px;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) 0;
  }
  .config-row label { font-size: var(--font-size-sm); color: var(--color-text-secondary); }
  .input-field {
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-2);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .save-row { display: flex; align-items: center; gap: var(--spacing-3); padding-top: var(--spacing-3); }
  .save-btn {
    background: var(--color-interactive-primary);
    color: var(--color-text-inverse);
    border: none;
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-4);
    cursor: pointer;
    font-size: var(--font-size-sm);
    font-weight: 500;
  }
  .save-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .save-success { font-size: var(--font-size-sm); color: var(--color-severity-normal); }

  .error-msg { color: var(--color-severity-hard); font-size: var(--font-size-sm); padding: var(--spacing-3); background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-hard); border-radius: var(--radius-sm); margin-bottom: var(--spacing-3); }
  .mono { font-family: var(--font-family-mono); font-variant-numeric: tabular-nums; }
</style>
