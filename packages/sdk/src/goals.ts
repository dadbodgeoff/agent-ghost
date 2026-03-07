import type { GhostRequestFn } from './client.js';

// ── Types ──

export interface Proposal {
  id: string;
  agent_id: string;
  session_id: string;
  proposer_type: 'agent' | 'human';
  operation: string;
  target_type: string;
  decision: 'approved' | 'rejected' | null;
  dimension_scores: Record<string, number>;
  flags: string[];
  created_at: string;
  resolved_at: string | null;
}

export interface ProposalDetail extends Proposal {
  content: Record<string, unknown>;
  cited_memory_ids: string[];
  resolver: string | null;
  denial_reason?: string;
}

export interface ListGoalsParams {
  status?: 'pending' | 'approved' | 'rejected';
  agent_id?: string;
  page?: number;
  page_size?: number;
}

export interface ListGoalsResult {
  proposals: Proposal[];
  page: number;
  page_size: number;
  total: number;
}

// ── API ──

export class GoalsAPI {
  constructor(private request: GhostRequestFn) {}

  /** List goal proposals with optional filtering. */
  async list(params?: ListGoalsParams): Promise<ListGoalsResult> {
    const query = new URLSearchParams();
    if (params?.status) query.set('status', params.status);
    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.page !== undefined) query.set('page', String(params.page));
    if (params?.page_size !== undefined) query.set('page_size', String(params.page_size));
    const qs = query.toString();
    return this.request<ListGoalsResult>('GET', `/api/goals${qs ? `?${qs}` : ''}`);
  }

  /** Get a single proposal with full detail. */
  async get(id: string): Promise<ProposalDetail> {
    return this.request<ProposalDetail>('GET', `/api/goals/${encodeURIComponent(id)}`);
  }

  /** Approve a pending proposal. */
  async approve(id: string): Promise<{ status: 'approved'; id: string }> {
    return this.request<{ status: 'approved'; id: string }>(
      'POST',
      `/api/goals/${encodeURIComponent(id)}/approve`,
    );
  }

  /** Reject a pending proposal. */
  async reject(id: string): Promise<{ status: 'rejected'; id: string }> {
    return this.request<{ status: 'rejected'; id: string }>(
      'POST',
      `/api/goals/${encodeURIComponent(id)}/reject`,
    );
  }
}
