import type { GhostRequestFn } from './client.js';
import type { AutonomyStatus } from './autonomy.js';

// ── Types ──

export interface HealthStatus {
  status: 'alive' | 'unavailable';
  state: 'Healthy' | 'Degraded' | 'Recovering' | 'Initializing' | 'ShuttingDown' | 'FatalError';
  platform_killed: boolean;
  autonomy?: AutonomyStatus;
  convergence_monitor?: {
    connected: boolean;
  };
  convergence_protection?: {
    execution_mode: string;
    stale_after_secs: number;
    agents: {
      healthy: number;
      missing: number;
      stale: number;
      corrupted: number;
    };
  };
  distributed_kill?: {
    enabled: boolean;
    status: string;
    node_id?: string;
    closed_at?: string;
    close_reason?: string;
    acked_nodes?: string[];
    chain_length?: number;
  };
  speculative_context?: {
    enabled: boolean;
    mode: string;
    shadow_mode: boolean;
    outstanding_entries: number;
    pending_tokens: number;
  };
}

export interface ReadyStatus {
  status: 'ready' | 'not_ready';
  state: string;
}

// ── API ──

export class HealthAPI {
  constructor(private request: GhostRequestFn) {}

  /** Check if the gateway is alive. */
  async check(): Promise<HealthStatus> {
    return this.request<HealthStatus>('GET', '/api/health');
  }

  /** Check if the gateway is ready to accept requests. */
  async ready(): Promise<ReadyStatus> {
    return this.request<ReadyStatus>('GET', '/api/ready');
  }
}
