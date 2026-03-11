import type { GhostRequestFn, GhostRequestOptions } from './client.js';

// ── Types ──

export interface DistributedKillStatus {
  enabled: boolean;
  status: string;
  authoritative: boolean;
  resume_permitted?: boolean;
  node_id?: string;
  closed_at?: string | null;
  close_reason?: string | null;
  acked_nodes?: string[] | number;
  chain_length?: number;
  reason?: string | null;
  error?: string;
}

export interface ConvergenceProtectionSummary {
  execution_mode: 'disabled' | 'allow' | 'block';
  stale_after_secs: number;
  agents: {
    healthy: number;
    missing: number;
    stale: number;
    corrupted: number;
  };
}

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
  convergence_protection?: ConvergenceProtectionSummary;
  distributed_kill?: DistributedKillStatus;
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

export interface SandboxReview {
  id: string;
  agent_id: string;
  session_id: string;
  execution_id?: string | null;
  route_kind?: string | null;
  tool_name: string;
  violation_reason: string;
  sandbox_mode: string;
  status: 'pending' | 'approved' | 'rejected' | 'expired';
  resolution_note?: string | null;
  resolved_by?: string | null;
  requested_at: string;
  resolved_at?: string | null;
}

export interface SandboxReviewListParams {
  status?: 'pending' | 'approved' | 'rejected' | 'expired';
  agent_id?: string;
  limit?: number;
}

export interface SandboxReviewDecisionResult {
  review_id: string;
  status: 'approved' | 'rejected';
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

  async listSandboxReviews(
    params: SandboxReviewListParams = {},
  ): Promise<{ reviews: SandboxReview[] }> {
    const search = new URLSearchParams();
    if (params.status) search.set('status', params.status);
    if (params.agent_id) search.set('agent_id', params.agent_id);
    if (params.limit != null) search.set('limit', String(params.limit));
    const suffix = search.toString() ? `?${search}` : '';
    return this.request<{ reviews: SandboxReview[] }>(
      'GET',
      `/api/safety/sandbox-reviews${suffix}`,
    );
  }

  async approveSandboxReview(
    reviewId: string,
    note?: string,
    options?: GhostRequestOptions,
  ): Promise<SandboxReviewDecisionResult> {
    return this.request<SandboxReviewDecisionResult>(
      'POST',
      `/api/safety/sandbox-reviews/${encodeURIComponent(reviewId)}/approve`,
      note ? { note } : {},
      options,
    );
  }

  async rejectSandboxReview(
    reviewId: string,
    note?: string,
    options?: GhostRequestOptions,
  ): Promise<SandboxReviewDecisionResult> {
    return this.request<SandboxReviewDecisionResult>(
      'POST',
      `/api/safety/sandbox-reviews/${encodeURIComponent(reviewId)}/reject`,
      note ? { note } : {},
      options,
    );
  }
}
