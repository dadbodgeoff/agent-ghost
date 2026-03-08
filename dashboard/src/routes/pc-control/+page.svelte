<script lang="ts">
  /**
   * PC Control Dashboard (Phase 3, Task 3.2).
   * Configures and monitors PC control safety features.
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import type {
    PcControlActionLogEntry as ActionLogEntry,
    PcControlStatus,
    SafeZone,
  } from '@ghost/sdk';
  import { wsStore } from '$lib/stores/websocket.svelte';

  let status: PcControlStatus | null = $state(null);
  let actionLog: ActionLogEntry[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let logFilter = $state('');

  // Edit states
  let newApp = $state('');
  let newHotkey = $state('');

  // Safe zone editor
  let svgEl = $state<SVGSVGElement | null>(null);
  let drawing = $state(false);
  let drawStart = $state({ x: 0, y: 0 });
  let drawCurrent = $state({ x: 0, y: 0 });
  let newZoneLabel = $state('');
  const SCREEN_W = 640;
  const SCREEN_H = 400;

  async function loadStatus() {
    try {
      const client = await getGhostClient();
      status = await client.pcControl.getStatus();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load PC control status';
    }
    loading = false;
  }

  async function loadActionLog() {
    try {
      const client = await getGhostClient();
      const data = await client.pcControl.listActions(100);
      actionLog = data?.actions ?? [];
    } catch { /* non-fatal */ }
  }

  async function toggleEnabled() {
    if (!status) return;
    try {
      const client = await getGhostClient();
      status = await client.pcControl.updateStatus(!status.enabled);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to toggle PC control';
    }
  }

  async function addApp() {
    if (!newApp.trim() || !status) return;
    try {
      const apps = [...status.allowed_apps, newApp.trim()];
      const client = await getGhostClient();
      status = await client.pcControl.setAllowedApps(apps);
      newApp = '';
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to add app';
    }
  }

  async function removeApp(app: string) {
    if (!status) return;
    try {
      const apps = status.allowed_apps.filter(a => a !== app);
      const client = await getGhostClient();
      status = await client.pcControl.setAllowedApps(apps);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to remove app';
    }
  }

  async function addHotkey() {
    if (!newHotkey.trim() || !status) return;
    try {
      const hotkeys = [...status.blocked_hotkeys, newHotkey.trim()];
      const client = await getGhostClient();
      status = await client.pcControl.setBlockedHotkeys(hotkeys);
      newHotkey = '';
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to add hotkey';
    }
  }

  async function removeHotkey(key: string) {
    if (!status) return;
    try {
      const hotkeys = status.blocked_hotkeys.filter(h => h !== key);
      const client = await getGhostClient();
      status = await client.pcControl.setBlockedHotkeys(hotkeys);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to remove hotkey';
    }
  }

  async function addSafeZone(zone: SafeZone) {
    if (!status) return;
    try {
      const zones = [...status.safe_zones, zone];
      const client = await getGhostClient();
      status = await client.pcControl.setSafeZones(zones);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to add safe zone';
    }
  }

  async function removeSafeZone(idx: number) {
    if (!status) return;
    try {
      const zones = status.safe_zones.filter((_, i) => i !== idx);
      const client = await getGhostClient();
      status = await client.pcControl.setSafeZones(zones);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to remove safe zone';
    }
  }

  function handleMouseDown(e: MouseEvent) {
    if (!svgEl) return;
    const rect = svgEl.getBoundingClientRect();
    drawStart = { x: e.clientX - rect.left, y: e.clientY - rect.top };
    drawCurrent = { ...drawStart };
    drawing = true;
  }

  function handleMouseMove(e: MouseEvent) {
    if (!drawing || !svgEl) return;
    const rect = svgEl.getBoundingClientRect();
    drawCurrent = { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  function handleMouseUp() {
    if (!drawing) return;
    drawing = false;
    const x = Math.min(drawStart.x, drawCurrent.x);
    const y = Math.min(drawStart.y, drawCurrent.y);
    const w = Math.abs(drawCurrent.x - drawStart.x);
    const h = Math.abs(drawCurrent.y - drawStart.y);
    if (w > 10 && h > 10) {
      const label = newZoneLabel.trim() || `Zone ${(status?.safe_zones.length ?? 0) + 1}`;
      addSafeZone({ x: Math.round(x), y: Math.round(y), width: Math.round(w), height: Math.round(h), label });
      newZoneLabel = '';
    }
  }

  function budgetPercent(used: number, max: number): number {
    if (max === 0) return 0;
    return Math.min(100, (used / max) * 100);
  }

  function cbStateLabel(state: string): string {
    switch (state) {
      case 'closed': return 'Closed (Normal)';
      case 'open': return 'Open (Blocked)';
      case 'half_open': return 'Half-Open (Testing)';
      default: return state;
    }
  }

  function cbStateColor(state: string): string {
    switch (state) {
      case 'closed': return 'var(--color-severity-normal)';
      case 'open': return 'var(--color-severity-hard)';
      case 'half_open': return 'var(--color-severity-soft)';
      default: return 'var(--color-text-muted)';
    }
  }

  let filteredLog = $derived(
    logFilter
      ? actionLog.filter(a => a.action_type.includes(logFilter) || a.target.includes(logFilter))
      : actionLog
  );

  onMount(() => {
    loadStatus();
    loadActionLog();
    const unsub = wsStore.on('AgentStateChange', () => { loadStatus(); });
    const unsubResync = wsStore.onResync(() => {
      loadStatus();
      loadActionLog();
    });
    return () => {
      unsub();
      unsubResync();
    };
  });
</script>

<h1 class="page-title">PC Control</h1>

{#if error}
  <div class="error-banner" role="alert">
    <span>{error}</span>
    <button onclick={() => (error = '')}>Dismiss</button>
  </div>
{/if}

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else if !status}
  <div class="empty-state">
    <p>PC Control data unavailable.</p>
    <button onclick={loadStatus}>Retry</button>
  </div>
{:else}
  <!-- Status Overview -->
  <section class="card">
    <div class="section-header">
      <h2>Status</h2>
      <button
        class="toggle-btn"
        class:enabled={status.enabled}
        onclick={toggleEnabled}
        aria-label="Toggle PC control"
      >
        {status.enabled ? 'Enabled' : 'Disabled'}
      </button>
    </div>
    <div class="status-row">
      <div class="status-item">
        <span class="status-label">Circuit Breaker</span>
        <span class="status-value" style="color: {cbStateColor(status.circuit_breaker_state)}">
          {cbStateLabel(status.circuit_breaker_state)}
        </span>
      </div>
    </div>
  </section>

  <!-- Action Budget -->
  <section class="card">
    <h2>Action Budget</h2>
    <div class="budget-grid">
      <div class="budget-item">
        <span class="budget-label">Per Minute</span>
        <div class="budget-bar-track">
          <div
            class="budget-bar-fill"
            class:budget-warning={budgetPercent(status.action_budget.used_this_minute, status.action_budget.max_per_minute) > 80}
            style="width: {budgetPercent(status.action_budget.used_this_minute, status.action_budget.max_per_minute)}%"
          ></div>
        </div>
        <span class="budget-count">{status.action_budget.used_this_minute} / {status.action_budget.max_per_minute}</span>
      </div>
      <div class="budget-item">
        <span class="budget-label">Per Hour</span>
        <div class="budget-bar-track">
          <div
            class="budget-bar-fill"
            class:budget-warning={budgetPercent(status.action_budget.used_this_hour, status.action_budget.max_per_hour) > 80}
            style="width: {budgetPercent(status.action_budget.used_this_hour, status.action_budget.max_per_hour)}%"
          ></div>
        </div>
        <span class="budget-count">{status.action_budget.used_this_hour} / {status.action_budget.max_per_hour}</span>
      </div>
    </div>
  </section>

  <!-- Application Allowlist -->
  <section class="card">
    <h2>Application Allowlist</h2>
    <div class="tag-list">
      {#each status.allowed_apps as app}
        <span class="tag">
          {app}
          <button class="tag-remove" onclick={() => removeApp(app)} aria-label="Remove {app}">x</button>
        </span>
      {/each}
    </div>
    <div class="add-row">
      <input type="text" bind:value={newApp} placeholder="Application name" onkeydown={(e) => e.key === 'Enter' && addApp()} />
      <button class="btn-sm" onclick={addApp}>Add</button>
    </div>
  </section>

  <!-- Safe Zone Editor -->
  <section class="card">
    <h2>Safe Zones</h2>
    <p class="hint">Click and drag to draw a safe zone on the screen preview.</p>
    <div class="zone-editor">
      <div class="zone-label-row">
        <input type="text" bind:value={newZoneLabel} placeholder="Zone label (optional)" />
      </div>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <svg
        bind:this={svgEl}
        class="zone-canvas"
        width={SCREEN_W}
        height={SCREEN_H}
        viewBox="0 0 {SCREEN_W} {SCREEN_H}"
        onmousedown={handleMouseDown}
        onmousemove={handleMouseMove}
        onmouseup={handleMouseUp}
      >
        <rect x="0" y="0" width={SCREEN_W} height={SCREEN_H} fill="var(--color-bg-elevated-2)" stroke="var(--color-border-default)" rx="4" />
        {#each status.safe_zones as zone, i}
          <g>
            <rect
              x={zone.x} y={zone.y} width={zone.width} height={zone.height}
              fill="var(--color-interactive-primary)" fill-opacity="0.15"
              stroke="var(--color-interactive-primary)" stroke-width="2" rx="2"
            />
            <text x={zone.x + 4} y={zone.y + 14} fill="var(--color-interactive-primary)" font-size="11">{zone.label}</text>
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <text
              x={zone.x + zone.width - 12} y={zone.y + 14}
              fill="var(--color-severity-hard)" font-size="11" cursor="pointer"
              onclick={(e: MouseEvent) => { e.stopPropagation(); removeSafeZone(i); }}
            >x</text>
          </g>
        {/each}
        {#if drawing}
          <rect
            x={Math.min(drawStart.x, drawCurrent.x)}
            y={Math.min(drawStart.y, drawCurrent.y)}
            width={Math.abs(drawCurrent.x - drawStart.x)}
            height={Math.abs(drawCurrent.y - drawStart.y)}
            fill="var(--color-interactive-primary)" fill-opacity="0.1"
            stroke="var(--color-interactive-primary)" stroke-dasharray="4"
          />
        {/if}
      </svg>
    </div>
  </section>

  <!-- Blocked Hotkeys -->
  <section class="card">
    <h2>Blocked Hotkeys</h2>
    <div class="tag-list">
      {#each status.blocked_hotkeys as key}
        <span class="tag">
          <kbd>{key}</kbd>
          <button class="tag-remove" onclick={() => removeHotkey(key)} aria-label="Remove {key}">x</button>
        </span>
      {/each}
    </div>
    <div class="add-row">
      <input type="text" bind:value={newHotkey} placeholder="e.g. Cmd+Q" onkeydown={(e) => e.key === 'Enter' && addHotkey()} />
      <button class="btn-sm" onclick={addHotkey}>Add</button>
    </div>
  </section>

  <!-- Action Log -->
  <section class="card">
    <div class="section-header">
      <h2>Action Log</h2>
      <input
        type="text"
        class="log-filter"
        bind:value={logFilter}
        placeholder="Filter actions…"
      />
    </div>
    {#if filteredLog.length === 0}
      <p class="no-data">No actions recorded.</p>
    {:else}
      <div class="log-table">
        <div class="log-header">
          <span>Time</span>
          <span>Action</span>
          <span>Target</span>
          <span>Result</span>
        </div>
        {#each filteredLog as entry (entry.id)}
          <div class="log-row">
            <span class="log-time">{new Date(entry.timestamp).toLocaleTimeString()}</span>
            <span>{entry.action_type}</span>
            <span class="log-target">{entry.target}</span>
            <span class="log-result" class:log-fail={entry.result === 'blocked'}>{entry.result}</span>
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  .card {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
    margin-bottom: var(--spacing-4);
  }

  .card h2 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-3);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-3);
  }

  .section-header h2 { margin-bottom: 0; }

  .toggle-btn {
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .toggle-btn.enabled {
    background: var(--color-severity-normal);
    color: var(--color-text-inverse);
    border-color: var(--color-severity-normal);
  }

  .status-row {
    display: flex;
    gap: var(--spacing-6);
  }

  .status-item {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .status-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .status-value {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
  }

  .budget-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--spacing-4);
  }

  .budget-item {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .budget-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .budget-bar-track {
    height: 8px;
    background: var(--color-bg-elevated-1);
    border-radius: 4px;
    overflow: hidden;
  }

  .budget-bar-fill {
    height: 100%;
    background: var(--color-interactive-primary);
    border-radius: 4px;
    transition: width 0.3s ease;
  }

  .budget-bar-fill.budget-warning {
    background: var(--color-severity-active);
  }

  .budget-count {
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    color: var(--color-text-muted);
  }

  .tag-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-2);
    margin-bottom: var(--spacing-3);
  }

  .tag {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-1);
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
  }

  .tag kbd {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .tag-remove {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-xs);
    padding: 0 2px;
  }

  .tag-remove:hover { color: var(--color-severity-hard); }

  .add-row {
    display: flex;
    gap: var(--spacing-2);
  }

  .add-row input {
    flex: 1;
    padding: var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .btn-sm {
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .zone-editor { margin-top: var(--spacing-2); }

  .zone-label-row {
    margin-bottom: var(--spacing-2);
  }

  .zone-label-row input {
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    width: 200px;
  }

  .zone-canvas {
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    cursor: crosshair;
    display: block;
    max-width: 100%;
  }

  .hint {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-2);
  }

  .log-filter {
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-xs);
    width: 180px;
  }

  .log-table {
    max-height: 300px;
    overflow-y: auto;
  }

  .log-header, .log-row {
    display: grid;
    grid-template-columns: 100px 1fr 1fr 80px;
    gap: var(--spacing-2);
    padding: var(--spacing-1) 0;
    font-size: var(--font-size-xs);
  }

  .log-header {
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border-subtle);
    padding-bottom: var(--spacing-2);
    margin-bottom: var(--spacing-1);
  }

  .log-time { font-family: var(--font-family-mono); color: var(--color-text-muted); }
  .log-target { color: var(--color-text-muted); overflow: hidden; text-overflow: ellipsis; }
  .log-result { font-weight: var(--font-weight-semibold); }
  .log-result.log-fail { color: var(--color-severity-hard); }

  .no-data {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    text-align: center;
    padding: var(--spacing-4);
  }

  .error-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-severity-hard-bg, rgba(255, 0, 0, 0.1));
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
    font-size: var(--font-size-sm);
    color: var(--color-severity-hard);
  }

  .error-banner button {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: var(--font-size-xs);
    text-decoration: underline;
  }

  .empty-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .empty-state button {
    margin-top: var(--spacing-4);
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
  }

  .skeleton-block {
    height: 200px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }
</style>
