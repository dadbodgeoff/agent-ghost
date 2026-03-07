import type { GhostRequestFn, GhostRequestOptions } from './client.js';

// ── Types ──

export interface SafetyStatus {
  platform_level?: string;
  platform_killed: boolean;
  state?: string;
  per_agent?: Record<
    string,
    {
      level: string;
      activated_at?: string;
      trigger?: string;
    }
  >;
  activated_at?: string;
  trigger?: string;
  distributed_gate?: {
    state: string;
    node_id: string;
    closed_at?: string;
    close_reason?: string;
    acked_nodes: string[] | number;
    chain_length: number;
  };
}

export interface KillAllResult {
  status: 'kill_all_activated';
  reason: string;
  initiated_by: string;
}

export interface PauseResult {
  status: 'paused';
  agent_id: string;
  reason: string;
}

export interface ResumeResult {
  status: 'resumed';
  agent_id: string;
  heightened_monitoring: boolean;
  monitoring_duration_hours: number;
}

export interface QuarantineResult {
  status: 'quarantined';
  agent_id: string;
  reason: string;
  resume_requires: string;
}

export interface ResumeParams {
  level?: 'PAUSE' | 'QUARANTINE';
  forensic_reviewed?: boolean;
  second_confirmation?: boolean;
}

// ── API ──

export class SafetyAPI {
  constructor(private request: GhostRequestFn) {}

  /** Get platform and per-agent safety status. */
  async status(): Promise<SafetyStatus> {
    return this.request<SafetyStatus>('GET', '/api/safety/status');
  }

  /** Activate platform-wide kill switch. */
  async killAll(
    reason: string,
    initiatedBy: string,
    options?: GhostRequestOptions,
  ): Promise<KillAllResult> {
    return this.request<KillAllResult>('POST', '/api/safety/kill-all', {
      reason,
      initiated_by: initiatedBy,
    }, options);
  }

  /** Pause a specific agent. */
  async pause(
    agentId: string,
    reason: string,
    options?: GhostRequestOptions,
  ): Promise<PauseResult> {
    return this.request<PauseResult>(
      'POST',
      `/api/safety/pause/${encodeURIComponent(agentId)}`,
      { reason },
      options,
    );
  }

  /** Resume a paused or quarantined agent. */
  async resume(
    agentId: string,
    params?: ResumeParams,
    options?: GhostRequestOptions,
  ): Promise<ResumeResult> {
    return this.request<ResumeResult>(
      'POST',
      `/api/safety/resume/${encodeURIComponent(agentId)}`,
      params ?? {},
      options,
    );
  }

  /** Quarantine an agent (requires forensic review to resume). */
  async quarantine(
    agentId: string,
    reason: string,
    options?: GhostRequestOptions,
  ): Promise<QuarantineResult> {
    return this.request<QuarantineResult>(
      'POST',
      `/api/safety/quarantine/${encodeURIComponent(agentId)}`,
      { reason },
      options,
    );
  }
}
