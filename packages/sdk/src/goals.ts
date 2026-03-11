import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

// ── Types ──

export type Proposal = Omit<
  components['schemas']['GoalProposalSummary'],
  'decision' | 'resolved_at'
> & {
  decision: string | null;
  resolved_at: string | null;
};
export type GoalProposalTransition = Omit<
  components['schemas']['GoalProposalTransition'],
  | 'actor_id'
  | 'reason_code'
  | 'rationale'
  | 'expected_state'
  | 'expected_revision'
  | 'operation_id'
  | 'request_id'
  | 'idempotency_key'
> & {
  actor_id: string | null;
  reason_code: string | null;
  rationale: string | null;
  expected_state: string | null;
  expected_revision: string | null;
  operation_id: string | null;
  request_id: string | null;
  idempotency_key: string | null;
};
export type ProposalDetail = Omit<
  components['schemas']['GoalProposalDetail'],
  | 'decision'
  | 'resolved_at'
  | 'resolver'
  | 'transition_history'
  | 'content'
> & {
  decision: string | null;
  resolved_at: string | null;
  resolver: string | null;
  content: Record<string, unknown>;
  transition_history?: GoalProposalTransition[];
};

export interface GoalDecisionRequest {
  expectedState: string;
  expectedLineageId: string;
  expectedSubjectKey: string;
  expectedReviewedRevision: string;
  rationale?: string;
}

export type GoalDecisionResult =
  operations['approve_goal']['responses'][200]['content']['application/json'];
export type ListGoalsParams = NonNullable<operations['list_goals']['parameters']['query']>;
export type ListGoalsResult = Omit<
  operations['list_goals']['responses'][200]['content']['application/json'],
  'proposals'
> & {
  proposals: Proposal[];
};
export type ActiveGoal = components['schemas']['ActiveGoalSummary'];
export type ListActiveGoalsResult = {
  goals: ActiveGoal[];
  total: number;
};

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

  /** List the canonical active goal set. */
  async listActive(params?: Omit<ListGoalsParams, 'status'>): Promise<ListActiveGoalsResult> {
    const query = new URLSearchParams();
    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.page !== undefined) query.set('page', String(params.page));
    if (params?.page_size !== undefined) query.set('page_size', String(params.page_size));
    const qs = query.toString();
    return this.request<ListActiveGoalsResult>('GET', `/api/goals/active${qs ? `?${qs}` : ''}`);
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
  ): Promise<GoalDecisionResult> {
    return this.request<GoalDecisionResult>(
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
  ): Promise<GoalDecisionResult> {
    return this.request<GoalDecisionResult>(
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
