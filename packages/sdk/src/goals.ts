import type { GhostRequestFn, GhostRequestOptions } from './client.js';

// ── Types ──

export interface Proposal {
  id: string;
  agent_id: string;
  session_id: string;
  proposer_type: 'agent' | 'human';
  operation: string;
  target_type: string;
  decision: string | null;
  dimension_scores: Record<string, number>;
  flags: string[];
  created_at: string;
  resolved_at: string | null;
  current_state?: string | null;
}

export interface ProposalDetail extends Proposal {
  content: Record<string, unknown>;
  cited_memory_ids: string[];
  resolver: string | null;
  denial_reason?: string;
  lineage_id?: string | null;
  subject_type?: string | null;
  subject_key?: string | null;
  reviewed_revision?: string | null;
  validation_disposition?: string | null;
  supersedes_proposal_id?: string | null;
  current_state?: string | null;
  transition_history?: GoalProposalTransition[];
}

export interface GoalProposalTransition {
  from_state: string | null;
  to_state: string;
  actor_type: string;
  actor_id: string | null;
  reason_code: string | null;
  rationale: string | null;
  expected_state: string | null;
  expected_revision: string | null;
  operation_id: string | null;
  request_id: string | null;
  idempotency_key: string | null;
  created_at: string;
}

export interface GoalDecisionRequest {
  expectedState: string;
  expectedLineageId: string;
  expectedSubjectKey: string;
  expectedReviewedRevision: string;
  rationale?: string;
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
  async approve(
    id: string,
    request: GoalDecisionRequest,
    options?: GhostRequestOptions,
  ): Promise<{ status: 'approved'; id: string }> {
    return this.request<{ status: 'approved'; id: string }>(
      'POST',
      `/api/goals/${encodeURIComponent(id)}/approve`,
      {
        expected_state: request.expectedState,
        expected_lineage_id: request.expectedLineageId,
        expected_subject_key: request.expectedSubjectKey,
        expected_reviewed_revision: request.expectedReviewedRevision,
        rationale: request.rationale,
      },
      options,
    );
  }

  /** Reject a pending proposal. */
  async reject(
    id: string,
    request: GoalDecisionRequest,
    options?: GhostRequestOptions,
  ): Promise<{ status: 'rejected'; id: string }> {
    return this.request<{ status: 'rejected'; id: string }>(
      'POST',
      `/api/goals/${encodeURIComponent(id)}/reject`,
      {
        expected_state: request.expectedState,
        expected_lineage_id: request.expectedLineageId,
        expected_subject_key: request.expectedSubjectKey,
        expected_reviewed_revision: request.expectedReviewedRevision,
        rationale: request.rationale,
      },
      options,
    );
  }
}
