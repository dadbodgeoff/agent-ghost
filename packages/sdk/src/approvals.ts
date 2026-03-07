import type { GhostRequestFn } from './client.js';

export type ApprovalType = 'tool_call' | 'spend' | 'escalation' | 'goal_change';
export type ApprovalStatus = 'pending' | 'approved' | 'denied';
export type ApprovalRiskLevel = 'low' | 'medium' | 'high';

export interface ApprovalDetails {
  tool?: string;
  args?: Record<string, unknown>;
  cost_estimate?: number;
  risk_level?: ApprovalRiskLevel;
}

export interface Approval {
  id: string;
  agent_id: string;
  agent_name: string;
  type: ApprovalType;
  description: string;
  details: ApprovalDetails;
  status: ApprovalStatus;
  created_at: string;
  decided_at?: string;
  decided_by?: string;
}

export interface ListApprovalsParams {
  status?: ApprovalStatus;
  page?: number;
  page_size?: number;
}

export interface ListApprovalsResult {
  proposals: Approval[];
  page: number;
  page_size: number;
  total: number;
}

export interface ApproveApprovalParams {
  modified_args?: Record<string, unknown>;
}

interface GoalSummary {
  id: string;
  agent_id: string;
  session_id: string;
  proposer_type: string;
  operation: string;
  target_type: string;
  decision: string | null;
  dimension_scores?: Record<string, number>;
  flags?: string[];
  created_at: string;
  resolved_at: string | null;
}

interface GoalDetail extends GoalSummary {
  content?: unknown;
  resolver?: string | null;
  denial_reason?: string;
}

interface GoalsListResult {
  proposals: GoalSummary[];
  page: number;
  page_size: number;
  total: number;
}

function normalizeGoalStatus(decision: string | null, resolvedAt: string | null): ApprovalStatus {
  if (!decision && !resolvedAt) {
    return 'pending';
  }
  return decision === 'rejected' ? 'denied' : 'approved';
}

function firstString(value: unknown, keys: string[]): string | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }

  const record = value as Record<string, unknown>;
  for (const key of keys) {
    const item = record[key];
    if (typeof item === 'string' && item.trim()) {
      return item;
    }
  }

  return undefined;
}

function firstObject(value: unknown, keys: string[]): Record<string, unknown> | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }

  const record = value as Record<string, unknown>;
  for (const key of keys) {
    const item = record[key];
    if (item && typeof item === 'object' && !Array.isArray(item)) {
      return item as Record<string, unknown>;
    }
  }

  return undefined;
}

function firstNumber(value: unknown, keys: string[]): number | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return undefined;
  }

  const record = value as Record<string, unknown>;
  for (const key of keys) {
    const item = record[key];
    if (typeof item === 'number' && Number.isFinite(item)) {
      return item;
    }
  }

  return undefined;
}

function inferRiskLevel(
  flags: string[] | undefined,
  dimensionScores: Record<string, number> | undefined,
): ApprovalRiskLevel | undefined {
  const normalizedFlags = (flags ?? []).map((flag) => flag.toLowerCase());
  if (normalizedFlags.some((flag) => flag.includes('high') || flag.includes('critical'))) {
    return 'high';
  }
  if (normalizedFlags.some((flag) => flag.includes('medium') || flag.includes('warn'))) {
    return 'medium';
  }

  const maxScore = Math.max(0, ...Object.values(dimensionScores ?? {}).filter((v) => Number.isFinite(v)));
  if (maxScore >= 0.8) {
    return 'high';
  }
  if (maxScore >= 0.5) {
    return 'medium';
  }
  if (maxScore > 0) {
    return 'low';
  }

  return undefined;
}

function inferApprovalType(
  operation: string,
  flags: string[] | undefined,
  content: unknown,
): ApprovalType {
  const op = operation.toLowerCase();
  const normalizedFlags = (flags ?? []).map((flag) => flag.toLowerCase());

  if (op.includes('goal')) {
    return 'goal_change';
  }
  if (
    normalizedFlags.some((flag) => flag.includes('cost') || flag.includes('spend')) ||
    firstNumber(content, ['cost_estimate', 'estimated_cost']) !== undefined
  ) {
    return 'spend';
  }
  if (normalizedFlags.some((flag) => flag.includes('escalat') || flag.includes('review'))) {
    return 'escalation';
  }
  return 'tool_call';
}

function describeApproval(
  operation: string,
  targetType: string,
  content: unknown,
): string {
  if (typeof content === 'string' && content.trim()) {
    return content;
  }

  const message = firstString(content, [
    'description',
    'summary',
    'reason',
    'goal',
    'goal_text',
    'text',
    'content',
    'message',
  ]);
  if (message) {
    return message;
  }

  return `${operation} on ${targetType}`;
}

function mapGoalToApproval(proposal: GoalSummary, detail?: GoalDetail): Approval {
  const content = detail?.content;
  const details: ApprovalDetails = {
    tool: firstString(content, ['tool', 'tool_name', 'name']),
    args: firstObject(content, ['args', 'tool_args', 'parameters', 'input']),
    cost_estimate: firstNumber(content, ['cost_estimate', 'estimated_cost']),
    risk_level: inferRiskLevel(proposal.flags, proposal.dimension_scores),
  };

  return {
    id: proposal.id,
    agent_id: proposal.agent_id,
    agent_name: proposal.agent_id,
    type: inferApprovalType(proposal.operation, proposal.flags, content),
    description: describeApproval(proposal.operation, proposal.target_type, content),
    details,
    status: normalizeGoalStatus(proposal.decision, proposal.resolved_at),
    created_at: proposal.created_at,
    decided_at: proposal.resolved_at ?? undefined,
    decided_by: detail?.resolver ?? undefined,
  };
}

export class ApprovalsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(params?: ListApprovalsParams): Promise<ListApprovalsResult> {
    const query = new URLSearchParams();
    if (params?.status) {
      query.set('status', params.status === 'denied' ? 'rejected' : params.status);
    }
    if (params?.page !== undefined) {
      query.set('page', String(params.page));
    }
    if (params?.page_size !== undefined) {
      query.set('page_size', String(params.page_size));
    }

    const qs = query.toString();
    const result = await this.request<GoalsListResult>(
      'GET',
      `/api/goals${qs ? `?${qs}` : ''}`,
    );

    const proposals = await Promise.all(
      result.proposals.map(async (proposal) => {
        try {
          const detail = await this.request<GoalDetail>(
            'GET',
            `/api/goals/${encodeURIComponent(proposal.id)}`,
          );
          return mapGoalToApproval(proposal, detail);
        } catch {
          return mapGoalToApproval(proposal);
        }
      }),
    );

    return {
      ...result,
      proposals,
    };
  }

  async approve(
    id: string,
    params?: ApproveApprovalParams,
  ): Promise<{ status: 'approved'; id: string }> {
    return this.request<{ status: 'approved'; id: string }>(
      'POST',
      `/api/goals/${encodeURIComponent(id)}/approve`,
      params,
    );
  }

  async deny(id: string): Promise<{ status: 'rejected'; id: string }> {
    return this.request<{ status: 'rejected'; id: string }>(
      'POST',
      `/api/goals/${encodeURIComponent(id)}/reject`,
    );
  }
}
