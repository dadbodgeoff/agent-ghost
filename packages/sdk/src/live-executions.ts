import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export interface LiveExecutionResult {
  execution_id: string;
  route_kind: string;
  state_version: number;
  status: string;
  operation_id: string;
  accepted_response: Record<string, unknown> | null;
  result_status_code: number | null;
  result_body: Record<string, unknown> | null;
  recovery_required: boolean;
  created_at: string;
  updated_at: string;
}

export interface LiveExecutionCancelResult {
  execution_id: string;
  route_kind: string;
  status: string;
  cancel_requested?: boolean;
  cancel_signal_sent?: boolean;
}

export class LiveExecutionsAPI {
  constructor(private request: GhostRequestFn) {}

  async get(executionId: string): Promise<LiveExecutionResult> {
    return this.request<LiveExecutionResult>(
      'GET',
      `/api/live-executions/${encodeURIComponent(executionId)}`,
    );
  }

  async cancel(
    executionId: string,
    options?: GhostRequestOptions,
  ): Promise<LiveExecutionCancelResult> {
    return this.request<LiveExecutionCancelResult>(
      'POST',
      `/api/live-executions/${encodeURIComponent(executionId)}/cancel`,
      undefined,
      options,
    );
  }
}
