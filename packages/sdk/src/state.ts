import type { GhostRequestFn } from './client.js';

export interface CrdtDelta {
  event_id: number;
  memory_id: string;
  event_type: string;
  delta: string;
  actor_id: string;
  recorded_at: string;
  event_hash: string;
  previous_hash: string;
}

export interface GetCrdtStateParams {
  memory_id?: string;
  limit?: number;
  offset?: number;
}

export interface CrdtStateResult {
  agent_id: string;
  deltas: CrdtDelta[];
  total: number;
  limit: number;
  offset: number;
  chain_valid: boolean;
}

export class StateAPI {
  constructor(private request: GhostRequestFn) {}

  async getCrdtState(agentId: string, params?: GetCrdtStateParams): Promise<CrdtStateResult> {
    const query = new URLSearchParams();

    if (params?.memory_id) query.set('memory_id', params.memory_id);
    if (params?.limit !== undefined) query.set('limit', String(params.limit));
    if (params?.offset !== undefined) query.set('offset', String(params.offset));

    const qs = query.toString();
    return this.request<CrdtStateResult>(
      'GET',
      `/api/state/crdt/${encodeURIComponent(agentId)}${qs ? `?${qs}` : ''}`,
    );
  }
}
