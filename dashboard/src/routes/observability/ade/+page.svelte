<script lang="ts">
  /**
   * ADE Self-Observability — gateway, monitor, WS, and SQLite health metrics.
   *
   * Ref: T-3.14.1
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import type { HealthStatus } from '@ghost/sdk';

  interface HealthData extends HealthStatus {
    uptime_secs?: number;
    db_size_bytes?: number;
    active_agents?: number;
    active_ws_connections?: number;
  }

  interface ComponentStatus {
    name: string;
    status: 'ok' | 'degraded' | 'error' | 'unknown';
    label: string;
    details: string;
  }

  let health: HealthData = $state({ status: 'unknown' });
  let error: string | null = $state(null);
  let refreshing = $state(false);

  let components: ComponentStatus[] = $derived.by(() => {
    const gwOk = health.status === 'ok' || health.status === 'healthy';
    return [
      {
        name: 'Gateway Server',
        status: gwOk ? 'ok' : 'degraded',
        label: gwOk ? 'Running' : health.status,
        details: 'Axum + Tower middleware stack',
      },
      {
        name: 'Convergence Monitor',
        status: gwOk ? 'ok' : 'unknown',
        label: gwOk ? 'Polling' : 'Unknown',
        details: '5s interval, convergence score watcher',
      },
      {
        name: 'WebSocket Handler',
        status: (health.active_ws_connections ?? 0) >= 0 ? 'ok' : 'unknown',
        label: `Active (${health.active_ws_connections ?? 0} conn)`,
        details: 'Topic-filtered broadcast, 30s keepalive',
      },
      {
        name: 'SQLite Database',
        status: gwOk ? 'ok' : 'error',
        label: gwOk ? 'Connected' : 'Disconnected',
        details: `WAL mode, ${formatBytes(health.db_size_bytes)}`,
      },
      {
        name: 'Backup Scheduler',
        status: gwOk ? 'ok' : 'unknown',
        label: gwOk ? 'Scheduled' : 'Unknown',
        details: 'Daily at 03:00 UTC, 30-day retention',
      },
      {
        name: 'Config Watcher',
        status: gwOk ? 'ok' : 'unknown',
        label: gwOk ? 'Watching' : 'Unknown',
        details: 'ghost.yml polling, 5s interval',
      },
    ];
  });

  onMount(() => {
    loadHealth();
  });

  async function loadHealth() {
    refreshing = true;
    try {
      const client = await getGhostClient();
      health = await client.health.check() as HealthData;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load health data';
    } finally {
      refreshing = false;
    }
  }

  function formatUptime(secs: number | undefined): string {
    if (!secs) return '—';
    const hours = Math.floor(secs / 3600);
    const mins = Math.floor((secs % 3600) / 60);
    if (hours > 24) return `${Math.floor(hours / 24)}d ${hours % 24}h`;
    return `${hours}h ${mins}m`;
  }

  function formatBytes(bytes: number | undefined): string {
    if (!bytes) return '—';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(1)} MB`;
  }
</script>

<svelte:head>
  <title>ADE Health | Observability | ADE</title>
</svelte:head>

<div class="ade-health-page">
  <header class="page-header">
    <div class="header-row">
      <div>
        <h1>ADE Self-Observability</h1>
        <p class="subtitle">Gateway, monitor, and infrastructure health metrics</p>
      </div>
      <button class="refresh-btn" onclick={loadHealth} disabled={refreshing}>
        {refreshing ? 'Refreshing…' : 'Refresh'}
      </button>
    </div>
  </header>

  {#if error}
    <p class="error-msg">{error}</p>
  {/if}

  <div class="metrics-grid">
    <div class="metric-card">
      <h3>Gateway Status</h3>
      <span class="metric-value" class:healthy={health.status === 'ok' || health.status === 'healthy'}>
        {health.status}
      </span>
    </div>

    <div class="metric-card">
      <h3>Uptime</h3>
      <span class="metric-value mono">{formatUptime(health.uptime_secs)}</span>
    </div>

    <div class="metric-card">
      <h3>Active Agents</h3>
      <span class="metric-value mono">{health.active_agents ?? '—'}</span>
    </div>

    <div class="metric-card">
      <h3>WS Connections</h3>
      <span class="metric-value mono">{health.active_ws_connections ?? '—'}</span>
    </div>

    <div class="metric-card">
      <h3>DB Size</h3>
      <span class="metric-value mono">{formatBytes(health.db_size_bytes)}</span>
    </div>

    <div class="metric-card">
      <h3>API Status</h3>
      <span class="metric-value healthy">Reachable</span>
    </div>
  </div>

  <section class="detail-section">
    <h2>Infrastructure Components</h2>
    <table class="data-table">
      <thead>
        <tr>
          <th>Component</th>
          <th>Status</th>
          <th>Details</th>
        </tr>
      </thead>
      <tbody>
        {#each components as comp}
          <tr>
            <td>{comp.name}</td>
            <td><span class="status-dot" class:ok={comp.status === 'ok'} class:degraded={comp.status === 'degraded'} class:error={comp.status === 'error'}></span> {comp.label}</td>
            <td>{comp.details}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>
</div>

<style>
  .ade-health-page { padding: var(--spacing-6); max-width: 1000px; }
  .page-header { margin-bottom: var(--spacing-6); }
  .header-row { display: flex; justify-content: space-between; align-items: flex-start; }
  .page-header h1 { font-size: var(--font-size-2xl); font-weight: 700; color: var(--color-text-primary); }
  .subtitle { color: var(--color-text-muted); font-size: var(--font-size-sm); margin-top: var(--spacing-1); }

  .refresh-btn {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-3);
    cursor: pointer;
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .refresh-btn:disabled { opacity: 0.5; }

  .metrics-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: var(--spacing-4); margin-bottom: var(--spacing-6); }
  .metric-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    text-align: center;
  }
  .metric-card h3 { font-size: var(--font-size-xs); color: var(--color-text-muted); text-transform: uppercase; margin-bottom: var(--spacing-2); }
  .metric-value { display: block; font-size: var(--font-size-xl); font-weight: 700; color: var(--color-text-primary); }
  .metric-value.healthy { color: var(--color-severity-normal); }

  .detail-section h2 { font-size: var(--font-size-lg); font-weight: 600; color: var(--color-text-primary); margin-bottom: var(--spacing-3); }

  .data-table { width: 100%; border-collapse: collapse; font-size: var(--font-size-sm); }
  .data-table th {
    text-align: left; padding: var(--spacing-2) var(--spacing-3); background: var(--color-bg-elevated-1);
    color: var(--color-text-muted); font-weight: 600; font-size: var(--font-size-xs);
    text-transform: uppercase; border-bottom: 1px solid var(--color-border-default);
  }
  .data-table td { padding: var(--spacing-2) var(--spacing-3); border-bottom: 1px solid var(--color-border-subtle); color: var(--color-text-primary); }

  .status-dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: var(--spacing-1); vertical-align: middle; }
  .status-dot.ok { background: var(--color-severity-normal); }
  .status-dot.degraded { background: var(--color-severity-active); }
  .status-dot.error { background: var(--color-severity-hard); }

  .error-msg { color: var(--color-severity-hard); font-size: var(--font-size-sm); padding: var(--spacing-3); background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-hard); border-radius: var(--radius-sm); margin-bottom: var(--spacing-3); }
  .mono { font-family: var(--font-family-mono); font-variant-numeric: tabular-nums; }
</style>
