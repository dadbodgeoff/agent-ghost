import type { GhostRequestFn } from './client.js';

// ── Types ──

export interface HealthStatus {
  status: 'alive' | 'unavailable';
  state: 'Healthy' | 'Degraded' | 'Recovering' | 'Initializing' | 'ShuttingDown' | 'FatalError';
  platform_killed: boolean;
  convergence_monitor?: {
    connected: boolean;
  };
  distributed_gate?: {
    state: string;
    node_id: string;
    closed_at?: string;
    close_reason?: string;
    acked_nodes: number;
    chain_length: number;
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
