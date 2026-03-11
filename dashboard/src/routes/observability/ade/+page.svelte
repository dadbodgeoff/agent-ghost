<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import type {
    AdeConvergenceProtectionSnapshot,
    AdeDistributedKillSnapshot,
    AdeObservabilitySnapshot,
  } from '@ghost/sdk';

  interface ComponentRow {
    name: string;
    status: string;
    summary: string;
    details: string;
  }

  const REFRESH_INTERVAL_MS = 15000;

  let snapshot: AdeObservabilitySnapshot | null = $state(null);
  let error: string | null = $state(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let loadedAt: string | null = $state(null);

  const componentRows: ComponentRow[] = $derived.by(() => {
    if (!snapshot) {
      return [];
    }

    return [
      {
        name: 'Gateway Server',
        status: snapshot.status,
        summary: `${snapshot.gateway.liveness} / ${snapshot.gateway.readiness}`,
        details: `State ${snapshot.gateway.state}, uptime ${formatDuration(snapshot.gateway.uptime_seconds)}`,
      },
      {
        name: 'Convergence Monitor',
        status: snapshot.monitor.status,
        summary: snapshot.monitor.connected ? 'Connected' : 'Disconnected',
        details: [
          snapshot.monitor.enabled ? 'enabled' : 'disabled',
          snapshot.monitor.agent_count != null ? `${snapshot.monitor.agent_count} agents` : null,
          snapshot.monitor.event_count != null ? `${snapshot.monitor.event_count} events` : null,
        ]
          .filter(Boolean)
          .join(' | '),
      },
      {
        name: 'Agent Registry',
        status: snapshot.agents.active_count > 0 ? 'healthy' : 'degraded',
        summary: `${snapshot.agents.active_count} active / ${snapshot.agents.registered_count} registered`,
        details: 'Live agent registry and operational state broadcast surface',
      },
      {
        name: 'WebSocket Transport',
        status: snapshot.websocket.status,
        summary: `${snapshot.websocket.active_connections} live connections`,
        details: `Per-IP limit ${snapshot.websocket.per_ip_limit}`,
      },
      {
        name: 'SQLite Database',
        status: snapshot.database.status,
        summary: snapshot.database.wal_mode ? 'WAL enabled' : 'WAL missing',
        details: `${formatBytes(snapshot.database.size_bytes)}${snapshot.database.path ? ` | ${snapshot.database.path}` : ''}`,
      },
      {
        name: 'Backup Scheduler',
        status: snapshot.backup_scheduler.status,
        summary: snapshot.backup_scheduler.enabled ? snapshot.backup_scheduler.schedule : 'disabled',
        details: snapshot.backup_scheduler.last_error
          ?? `Retention ${snapshot.backup_scheduler.retention_days} days`,
      },
      {
        name: 'Config Watcher',
        status: snapshot.config_watcher.status,
        summary: snapshot.config_watcher.mode ?? 'unknown',
        details: snapshot.config_watcher.last_error
          ?? snapshot.config_watcher.watched_path
          ?? 'No watched path reported',
      },
    ];
  });

  onMount(() => {
    void loadSnapshot(true);

    const timer = setInterval(() => {
      void loadSnapshot(false);
    }, REFRESH_INTERVAL_MS);

    const unsubResync = wsStore.onResync(() => {
      void loadSnapshot(false);
    });
    const unsubBackup = wsStore.on('BackupComplete', () => {
      void loadSnapshot(false);
    });
    const unsubAgent = wsStore.on('AgentOperationalStatusChanged', () => {
      void loadSnapshot(false);
    });
    const unsubChannel = wsStore.on('ChannelStatusChanged', () => {
      void loadSnapshot(false);
    });

    return () => {
      clearInterval(timer);
      unsubResync();
      unsubBackup();
      unsubAgent();
      unsubChannel();
    };
  });

  async function loadSnapshot(firstLoad: boolean) {
    if (firstLoad) {
      loading = true;
    } else {
      refreshing = true;
    }

    try {
      const client = await getGhostClient();
      snapshot = await client.observability.ade();
      loadedAt = new Date().toISOString();
      error = null;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load ADE observability snapshot';
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  function formatDuration(seconds: number | null | undefined): string {
    if (seconds == null) return '--';
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  }

  function formatBytes(bytes: number | null | undefined): string {
    if (bytes == null) return '--';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatTimestamp(value: string | null | undefined): string {
    if (!value) return '--';
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) return value;
    return parsed.toLocaleString();
  }

  function ageLabel(value: string | null | undefined): string {
    if (!value) return '--';
    const diffMs = Date.now() - new Date(value).getTime();
    if (!Number.isFinite(diffMs)) return '--';
    const diffSeconds = Math.max(0, Math.floor(diffMs / 1000));
    if (diffSeconds < 60) return `${diffSeconds}s ago`;
    const diffMinutes = Math.floor(diffSeconds / 60);
    if (diffMinutes < 60) return `${diffMinutes}m ago`;
    const diffHours = Math.floor(diffMinutes / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${Math.floor(diffHours / 24)}d ago`;
  }

  function tone(status: string | null | undefined): 'healthy' | 'degraded' | 'warning' | 'disabled' | 'unavailable' {
    switch (status) {
      case 'healthy':
      case 'running':
      case 'alive':
      case 'ready':
        return 'healthy';
      case 'degraded':
      case 'recovering':
        return 'degraded';
      case 'disabled':
      case 'gated':
        return 'disabled';
      case 'unreachable':
      case 'unavailable':
      case 'error':
      case 'fatal':
        return 'unavailable';
      default:
        return 'warning';
    }
  }

  function convergenceSummary(summary: AdeConvergenceProtectionSnapshot | undefined): string {
    if (!summary) return '--';
    return `${summary.execution_mode} mode | healthy ${summary.agents.healthy}, stale ${summary.agents.stale}, missing ${summary.agents.missing}, corrupted ${summary.agents.corrupted}`;
  }

  function distributedKillSummary(summary: AdeDistributedKillSnapshot | undefined): string {
    if (!summary) return '--';
    if (!summary.enabled) return summary.reason ?? summary.status;
    return [
      summary.status,
      summary.node_id ? `node ${summary.node_id}` : null,
      summary.chain_length != null ? `chain ${summary.chain_length}` : null,
    ]
      .filter(Boolean)
      .join(' | ');
  }
</script>

<svelte:head>
  <title>ADE Health | Observability | ADE</title>
</svelte:head>

<div class="ade-health-page">
  <div class="hero">
    <div>
      <div class="eyebrow">Canonical ADE snapshot</div>
      <h2>Control-plane health is now sourced from one endpoint.</h2>
      <p>
        Sampled {snapshot ? ageLabel(snapshot.sampled_at) : '--'}.
        {#if loadedAt}
          UI synced {ageLabel(loadedAt)}.
        {/if}
      </p>
    </div>
    <div class="hero-actions">
      {#if snapshot}
        <span class={`status-chip ${tone(snapshot.status)}`}>{snapshot.status}</span>
      {/if}
      <button class="refresh-btn" onclick={() => void loadSnapshot(false)} disabled={refreshing}>
        {refreshing ? 'Refreshing...' : 'Refresh'}
      </button>
    </div>
  </div>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  {#if snapshot?.stale}
    <div class="banner warning">
      Monitor data is stale. Last monitor computation: {formatTimestamp(snapshot.monitor.last_computation)}.
    </div>
  {/if}

  {#if loading && !snapshot}
    <div class="empty-state">Loading ADE observability snapshot...</div>
  {:else if snapshot}
    <div class="metrics-grid">
      <article class="metric-card">
        <span class="metric-label">Gateway</span>
        <strong>{snapshot.gateway.state}</strong>
        <span>{snapshot.gateway.liveness} / {snapshot.gateway.readiness}</span>
      </article>
      <article class="metric-card">
        <span class="metric-label">Uptime</span>
        <strong class="mono">{formatDuration(snapshot.gateway.uptime_seconds)}</strong>
        <span>Started {ageLabel(snapshot.sampled_at)}</span>
      </article>
      <article class="metric-card">
        <span class="metric-label">Agents</span>
        <strong class="mono">{snapshot.agents.active_count}</strong>
        <span>{snapshot.agents.registered_count} registered</span>
      </article>
      <article class="metric-card">
        <span class="metric-label">WebSocket</span>
        <strong class="mono">{snapshot.websocket.active_connections}</strong>
        <span>Limit {snapshot.websocket.per_ip_limit} per IP</span>
      </article>
      <article class="metric-card">
        <span class="metric-label">Database</span>
        <strong class="mono">{formatBytes(snapshot.database.size_bytes)}</strong>
        <span>{snapshot.database.wal_mode ? 'WAL enabled' : 'WAL missing'}</span>
      </article>
      <article class="metric-card">
        <span class="metric-label">Monitor</span>
        <strong>{snapshot.monitor.status}</strong>
        <span>{snapshot.monitor.connected ? 'connected' : 'disconnected'}</span>
      </article>
    </div>

    <div class="secondary-grid">
      <article class="detail-card">
        <h3>Convergence Protection</h3>
        <p>{convergenceSummary(snapshot.convergence_protection)}</p>
        <div class="detail-meta mono">stale after {snapshot.convergence_protection.stale_after_secs}s</div>
      </article>
      <article class="detail-card">
        <h3>Distributed Kill</h3>
        <p>{distributedKillSummary(snapshot.distributed_kill)}</p>
        <div class="detail-meta">{snapshot.distributed_kill.authoritative ? 'authoritative' : 'non-authoritative'}</div>
      </article>
      <article class="detail-card">
        <h3>Autonomy</h3>
        <p>{snapshot.autonomy.runtime_state} with {snapshot.autonomy.running_jobs} running jobs</p>
        <div class="detail-meta">
          due {snapshot.autonomy.due_jobs} | waiting {snapshot.autonomy.waiting_jobs} | paused {snapshot.autonomy.paused_jobs}
        </div>
      </article>
    </div>

    <section class="detail-section">
      <h3>Infrastructure Components</h3>
      <table class="data-table">
        <thead>
          <tr>
            <th>Component</th>
            <th>Status</th>
            <th>Summary</th>
            <th>Details</th>
          </tr>
        </thead>
        <tbody>
          {#each componentRows as row}
            <tr>
              <td>{row.name}</td>
              <td>
                <span class={`status-chip ${tone(row.status)}`}>{row.status}</span>
              </td>
              <td>{row.summary}</td>
              <td>{row.details}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>

    <section class="detail-section">
      <h3>Operational Details</h3>
      <div class="operational-grid">
        <article class="detail-card">
          <h4>Backups</h4>
          <p>Last success: {formatTimestamp(snapshot.backup_scheduler.last_success_at)}</p>
          <p>Last failure: {formatTimestamp(snapshot.backup_scheduler.last_failure_at)}</p>
          <p>Error: {snapshot.backup_scheduler.last_error ?? '--'}</p>
        </article>
        <article class="detail-card">
          <h4>Config Watcher</h4>
          <p>Mode: {snapshot.config_watcher.mode ?? '--'}</p>
          <p>Path: {snapshot.config_watcher.watched_path ?? '--'}</p>
          <p>Last reload: {formatTimestamp(snapshot.config_watcher.last_reload_at)}</p>
        </article>
        <article class="detail-card">
          <h4>Speculative Context</h4>
          <p>Mode: {snapshot.speculative_context.mode}</p>
          <p>Shadow mode: {snapshot.speculative_context.shadow_mode ? 'enabled' : 'disabled'}</p>
          <p>Outstanding: {snapshot.speculative_context.outstanding_entries}</p>
        </article>
      </div>
    </section>
  {/if}
</div>

<style>
  .ade-health-page {
    max-width: 1200px;
  }

  .hero {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-4);
    padding: var(--spacing-5);
    background: linear-gradient(135deg, color-mix(in srgb, var(--color-surface-selected) 60%, transparent), var(--color-bg-elevated-1));
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-lg);
    margin-bottom: var(--spacing-4);
  }

  .eyebrow {
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    margin-bottom: var(--spacing-2);
  }

  .hero h2 {
    margin: 0;
    font-size: var(--font-size-xl);
    color: var(--color-text-primary);
  }

  .hero p {
    margin: var(--spacing-2) 0 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .hero-actions {
    display: flex;
    align-items: flex-start;
    gap: var(--spacing-2);
  }

  .refresh-btn {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-3);
    cursor: pointer;
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }

  .refresh-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .banner {
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
    margin-bottom: var(--spacing-3);
    font-size: var(--font-size-sm);
  }

  .banner.error {
    background: color-mix(in srgb, var(--color-severity-hard) 12%, transparent);
    border: 1px solid var(--color-severity-hard);
    color: var(--color-text-primary);
  }

  .banner.warning {
    background: color-mix(in srgb, var(--color-severity-active) 12%, transparent);
    border: 1px solid var(--color-severity-active);
    color: var(--color-text-primary);
  }

  .empty-state {
    padding: var(--spacing-8);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .metrics-grid,
  .secondary-grid,
  .operational-grid {
    display: grid;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .metrics-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .secondary-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .operational-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .metric-card,
  .detail-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .metric-card {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .metric-label {
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .metric-card strong,
  .detail-card h3,
  .detail-card h4 {
    color: var(--color-text-primary);
  }

  .metric-card span:last-child,
  .detail-card p,
  .detail-meta {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin: 0;
  }

  .detail-section {
    margin-top: var(--spacing-5);
  }

  .detail-section h3 {
    font-size: var(--font-size-lg);
    color: var(--color-text-primary);
    margin-bottom: var(--spacing-3);
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .data-table th,
  .data-table td {
    padding: var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
    text-align: left;
    font-size: var(--font-size-sm);
  }

  .data-table th {
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    background: var(--color-bg-elevated-2);
  }

  .status-chip {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 96px;
    padding: 0.35rem 0.65rem;
    border-radius: 999px;
    font-size: var(--font-size-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .status-chip.healthy {
    background: color-mix(in srgb, var(--color-severity-normal) 18%, transparent);
    color: var(--color-severity-normal);
  }

  .status-chip.degraded,
  .status-chip.warning {
    background: color-mix(in srgb, var(--color-severity-active) 18%, transparent);
    color: var(--color-severity-active);
  }

  .status-chip.disabled {
    background: color-mix(in srgb, var(--color-text-muted) 18%, transparent);
    color: var(--color-text-muted);
  }

  .status-chip.unavailable {
    background: color-mix(in srgb, var(--color-severity-hard) 18%, transparent);
    color: var(--color-severity-hard);
  }

  .mono {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  @media (max-width: 900px) {
    .hero {
      flex-direction: column;
    }

    .hero-actions {
      justify-content: space-between;
    }

    .metrics-grid,
    .secondary-grid,
    .operational-grid {
      grid-template-columns: 1fr;
    }

    .data-table {
      display: block;
      overflow-x: auto;
    }
  }
</style>
