import type { GhostRequestFn, GhostRequestOptions } from './client.js';

// ── Types ──

export interface Agent {
  id: string;
  name: string;
  status: 'starting' | 'ready' | 'paused' | 'quarantined' | 'kill_all_blocked' | 'stopping' | 'stopped';
  lifecycle_state: 'starting' | 'ready' | 'stopping' | 'stopped';
  safety_state: 'normal' | 'paused' | 'quarantined' | 'kill_all_blocked';
  effective_state: 'starting' | 'ready' | 'paused' | 'quarantined' | 'kill_all_blocked' | 'stopping' | 'stopped';
  spending_cap: number;
  isolation?: 'in_process' | 'process' | 'container';
  capabilities?: string[];
  sandbox?: AgentSandboxConfig;
  sandbox_metrics?: AgentSandboxMetrics;
  action_policy?: AgentActionPolicy;
}

export interface AgentDetail extends Agent {
  has_keypair?: boolean;
}

export interface AgentActionPolicy {
  can_pause: boolean;
  can_quarantine: boolean;
  can_resume: boolean;
  can_delete: boolean;
  resume_kind?: 'pause' | 'quarantine' | null;
  requires_forensic_review: boolean;
  requires_second_confirmation: boolean;
  monitoring_duration_hours?: number | null;
}

export interface AgentSandboxConfig {
  enabled: boolean;
  mode: 'off' | 'read_only' | 'workspace_write' | 'strict';
  on_violation: 'warn' | 'pause' | 'quarantine' | 'kill_all';
  network_access: boolean;
  allowed_shell_prefixes: string[];
}

export interface AgentSandboxMetrics {
  pending_reviews: number;
  total_reviews: number;
  approved_reviews: number;
  rejected_reviews: number;
  expired_reviews: number;
  last_requested_at?: string | null;
}

export interface CreateAgentParams {
  name: string;
  spending_cap?: number;
  capabilities?: string[];
  skills?: string[];
  sandbox?: AgentSandboxConfig;
  generate_keypair?: boolean;
}

export interface UpdateAgentParams {
  spending_cap?: number;
  capabilities?: string[];
  sandbox?: AgentSandboxConfig;
}

export interface DeleteAgentResult {
  status: 'deleted';
  id: string;
  name: string;
}

export interface AgentAuditEntrySummary {
  id: string;
  timestamp: string;
  event_type: string;
  severity: string;
  details: string;
  agent_id: string;
  actor_id?: string | null;
}

export interface AgentCostSummary {
  agent_id: string;
  agent_name: string;
  daily_total: number;
  compaction_cost: number;
  spending_cap: number;
  cap_remaining: number;
  cap_utilization_pct: number;
}

export interface OverviewPanelStatus {
  state: 'ready' | 'empty' | 'unavailable' | 'error';
  message?: string | null;
}

export interface AgentOverviewPanelHealth {
  convergence: OverviewPanelStatus;
  cost: OverviewPanelStatus;
  recent_sessions: OverviewPanelStatus;
  recent_audit_entries: OverviewPanelStatus;
  crdt_summary: OverviewPanelStatus;
  integrity_summary: OverviewPanelStatus;
}

export interface AgentOverview {
  agent: AgentDetail;
  convergence?: {
    agent_id: string;
    agent_name: string;
    score: number;
    level: number;
    profile: string;
    signal_scores: Record<string, number>;
    computed_at?: string | null;
  } | null;
  cost?: AgentCostSummary | null;
  recent_sessions: Array<{
    session_id: string;
    agent_ids?: string[];
    started_at: string;
    last_event_at: string;
    event_count: number;
    chain_valid?: boolean;
    cumulative_cost?: number;
    branched_from?: string | null;
    agents?: string;
  }>;
  recent_audit_entries: AgentAuditEntrySummary[];
  crdt_summary?: {
    agent_id: string;
    deltas: Array<{
      event_id: number;
      memory_id: string;
      event_type: string;
      delta: string;
      actor_id: string;
      recorded_at: string;
      event_hash: string;
      previous_hash: string;
    }>;
    total: number;
    limit: number;
    offset: number;
    chain_valid: boolean;
  } | null;
  integrity_summary?: {
    agent_id: string;
    chain_type: string;
    chains: Record<string, unknown>;
  } | null;
  panel_health: AgentOverviewPanelHealth;
}

export interface GetAgentOverviewParams {
  sessions_limit?: number;
  audit_limit?: number;
  crdt_limit?: number;
}

// ── API ──

export class AgentsAPI {
  constructor(private request: GhostRequestFn) {}

  /** List all registered agents. */
  async list(): Promise<Agent[]> {
    return this.request<Agent[]>('GET', '/api/agents');
  }

  /** Create a new agent with optional keypair generation. */
  async create(params: CreateAgentParams, options?: GhostRequestOptions): Promise<AgentDetail> {
    return this.request<AgentDetail>('POST', '/api/agents', params, options);
  }

  /** Get one agent detail by ID or name. */
  async get(id: string): Promise<AgentDetail> {
    return this.request<AgentDetail>('GET', `/api/agents/${encodeURIComponent(id)}`);
  }

  /** Get the cohesive overview payload for the agent detail page. */
  async getOverview(id: string, params: GetAgentOverviewParams = {}): Promise<AgentOverview> {
    const query = new URLSearchParams();
    if (params.sessions_limit !== undefined) query.set('sessions_limit', String(params.sessions_limit));
    if (params.audit_limit !== undefined) query.set('audit_limit', String(params.audit_limit));
    if (params.crdt_limit !== undefined) query.set('crdt_limit', String(params.crdt_limit));
    const qs = query.toString();
    return this.request<AgentOverview>(
      'GET',
      `/api/agents/${encodeURIComponent(id)}/overview${qs ? `?${qs}` : ''}`,
    );
  }

  /** Update a live agent's runtime settings. */
  async update(id: string, params: UpdateAgentParams, options?: GhostRequestOptions): Promise<AgentDetail> {
    return this.request<AgentDetail>(
      'PATCH',
      `/api/agents/${encodeURIComponent(id)}`,
      params,
      options,
    );
  }

  /** Delete an agent by ID or name. */
  async delete(id: string, options?: GhostRequestOptions): Promise<DeleteAgentResult> {
    return this.request<DeleteAgentResult>(
      'DELETE',
      `/api/agents/${encodeURIComponent(id)}`,
      undefined,
      options,
    );
  }
}
