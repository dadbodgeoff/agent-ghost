import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export interface AutonomySaturationStatus {
  saturated: boolean;
  reserved_slots: number;
  global_concurrency: number;
  per_agent_concurrency: number;
  blocked_due_jobs: number;
  reason?: string;
}

export interface AutonomyStatus {
  deployment_mode: string;
  runtime_state: string;
  scheduler_running: boolean;
  worker_count: number;
  due_jobs: number;
  leased_jobs: number;
  running_jobs: number;
  waiting_jobs: number;
  paused_jobs: number;
  quarantined_jobs: number;
  manual_review_jobs: number;
  oldest_overdue_at?: string;
  last_successful_dispatch_at?: string;
  owner_identity: string;
  saturation: AutonomySaturationStatus;
}

export interface AutonomyJob {
  id: string;
  job_type: string;
  agent_id: string;
  workflow_id?: string;
  policy_scope: string;
  state: string;
  next_run_at: string;
  schedule_json: string;
  current_run_id?: string;
  overlap_policy: string;
  missed_run_policy: string;
  initiative_mode: string;
  approval_policy: string;
  manual_review_required: boolean;
  retry_count: number;
  retry_after?: string;
  last_heartbeat_at?: string;
  last_success_at?: string;
  last_failure_at?: string;
  terminal_reason?: string;
}

export interface AutonomyRun {
  id: string;
  job_id: string;
  attempt: number;
  state: string;
  trigger_source: string;
  due_at: string;
  started_at?: string;
  completed_at?: string;
  approval_state: string;
  side_effect_status: string;
  why_now_json: Record<string, unknown>;
  terminal_reason?: string;
  manual_review_required: boolean;
}

export interface AutonomyPolicyDocument {
  version: number;
  pause: boolean;
  draft_only: boolean;
  approval_required: boolean;
  quiet_hours?: {
    timezone: string;
    start_hour: number;
    end_hour: number;
  };
  initiative_budget: {
    max_daily_cost: number;
    max_risk_score: number;
    max_interruptions_per_day: number;
    max_novelty_score: number;
    min_trust_score: number;
  };
  retention_days: number;
}

export interface AutonomyPolicyResponse {
  scope_kind: string;
  scope_key: string;
  policy: AutonomyPolicyDocument;
}

export interface AutonomySuppression {
  id: string;
  scope_kind: string;
  scope_key: string;
  fingerprint: string;
  reason: string;
  created_by: string;
  created_at: string;
  expires_at?: string;
  active: boolean;
  metadata_json: Record<string, unknown>;
}

export interface AutonomySuppressionsResponse {
  suppressions: AutonomySuppression[];
}

export interface ApproveAutonomyRunResponse {
  run_id: string;
  approval_state: string;
  approval_expires_at: string;
}

export class AutonomyAPI {
  constructor(private request: GhostRequestFn) {}

  async status(): Promise<AutonomyStatus> {
    return this.request<AutonomyStatus>('GET', '/api/autonomy/status');
  }

  async listJobs(limit = 50): Promise<{ jobs: AutonomyJob[] }> {
    return this.request<{ jobs: AutonomyJob[] }>(
      'GET',
      `/api/autonomy/jobs?limit=${encodeURIComponent(String(limit))}`,
    );
  }

  async listRuns(limit = 50): Promise<{ runs: AutonomyRun[] }> {
    return this.request<{ runs: AutonomyRun[] }>(
      'GET',
      `/api/autonomy/runs?limit=${encodeURIComponent(String(limit))}`,
    );
  }

  async getGlobalPolicy(): Promise<AutonomyPolicyResponse> {
    return this.request<AutonomyPolicyResponse>('GET', '/api/autonomy/policies/global');
  }

  async updateGlobalPolicy(
    policy: AutonomyPolicyDocument,
    options?: GhostRequestOptions,
  ): Promise<AutonomyPolicyResponse> {
    return this.request<AutonomyPolicyResponse>(
      'PUT',
      '/api/autonomy/policies/global',
      { policy },
      options,
    );
  }

  async getAgentPolicy(agentId: string): Promise<AutonomyPolicyResponse> {
    return this.request<AutonomyPolicyResponse>(
      'GET',
      `/api/autonomy/policies/agents/${encodeURIComponent(agentId)}`,
    );
  }

  async updateAgentPolicy(
    agentId: string,
    policy: AutonomyPolicyDocument,
    options?: GhostRequestOptions,
  ): Promise<AutonomyPolicyResponse> {
    return this.request<AutonomyPolicyResponse>(
      'PUT',
      `/api/autonomy/policies/agents/${encodeURIComponent(agentId)}`,
      { policy },
      options,
    );
  }

  async suppress(
    body: {
      scope_kind: string;
      scope_key: string;
      fingerprint: string;
      reason: string;
      expires_at?: string;
      metadata?: Record<string, unknown>;
    },
    options?: GhostRequestOptions,
  ): Promise<AutonomySuppressionsResponse> {
    return this.request<AutonomySuppressionsResponse>(
      'POST',
      '/api/autonomy/suppressions',
      body,
      options,
    );
  }

  async approveRun(
    runId: string,
    ttlSeconds?: number,
    options?: GhostRequestOptions,
  ): Promise<ApproveAutonomyRunResponse> {
    return this.request<ApproveAutonomyRunResponse>(
      'POST',
      `/api/autonomy/runs/${encodeURIComponent(runId)}/approve`,
      { ttl_seconds: ttlSeconds },
      options,
    );
  }
}
