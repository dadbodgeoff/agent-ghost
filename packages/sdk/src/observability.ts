import type { GhostRequestFn } from './client.js';
import type { AutonomyStatus } from './autonomy.js';

export interface AdeConvergenceProtectionAgents {
  healthy: number;
  missing: number;
  stale: number;
  corrupted: number;
}

export interface AdeConvergenceProtectionSnapshot {
  execution_mode: string;
  stale_after_secs: number;
  agents: AdeConvergenceProtectionAgents;
}

export interface AdeDistributedKillSnapshot {
  enabled: boolean;
  status: string;
  authoritative: boolean;
  resume_permitted?: boolean;
  node_id?: string;
  closed_at?: string | null;
  close_reason?: string | null;
  acked_nodes?: string[];
  chain_length?: number;
  reason?: string | null;
  error?: string;
}

export interface AdeGatewaySnapshot {
  liveness: 'alive' | 'unavailable';
  readiness: 'ready' | 'not_ready';
  state: string;
  uptime_seconds: number;
  platform_killed: boolean;
}

export interface AdeMonitorSnapshot {
  enabled: boolean;
  connected: boolean;
  status: string;
  uptime_seconds?: number | null;
  agent_count?: number | null;
  event_count?: number | null;
  last_computation?: string | null;
  last_error?: string | null;
}

export interface AdeAgentSnapshot {
  active_count: number;
  registered_count: number;
}

export interface AdeWebSocketSnapshot {
  active_connections: number;
  per_ip_limit: number;
  status: string;
}

export interface AdeDatabaseSnapshot {
  path?: string | null;
  size_bytes?: number | null;
  wal_mode?: boolean | null;
  status: string;
  last_error?: string | null;
}

export interface AdeBackupSchedulerSnapshot {
  enabled: boolean;
  status: string;
  retention_days: number;
  schedule: string;
  last_success_at?: string | null;
  last_failure_at?: string | null;
  last_error?: string | null;
}

export interface AdeConfigWatcherSnapshot {
  enabled: boolean;
  status: string;
  watched_path?: string | null;
  mode?: string | null;
  last_reload_at?: string | null;
  last_error?: string | null;
}

export interface SpeculativeContextStatus {
  enabled: boolean;
  mode: string;
  shadow_mode: boolean;
  outstanding_entries: number;
  pending_tokens: number;
}

export interface AdeObservabilitySnapshot {
  sampled_at: string;
  stale: boolean;
  status: string;
  gateway: AdeGatewaySnapshot;
  monitor: AdeMonitorSnapshot;
  agents: AdeAgentSnapshot;
  websocket: AdeWebSocketSnapshot;
  database: AdeDatabaseSnapshot;
  backup_scheduler: AdeBackupSchedulerSnapshot;
  config_watcher: AdeConfigWatcherSnapshot;
  autonomy: AutonomyStatus;
  convergence_protection: AdeConvergenceProtectionSnapshot;
  distributed_kill: AdeDistributedKillSnapshot;
  speculative_context: SpeculativeContextStatus;
}

export class ObservabilityAPI {
  constructor(private request: GhostRequestFn) {}

  async ade(): Promise<AdeObservabilitySnapshot> {
    return this.request<AdeObservabilitySnapshot>('GET', '/api/observability/ade');
  }
}
