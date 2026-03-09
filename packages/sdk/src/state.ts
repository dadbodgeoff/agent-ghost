import type { GhostRequestFn } from './client.js';
import type { components, operations } from './generated-types.js';

export type CrdtDelta = components['schemas']['CrdtDelta'];
export type GetCrdtStateParams = NonNullable<
  operations['get_crdt_state']['parameters']['query']
>;
export type CrdtStateResult =
  operations['get_crdt_state']['responses'][200]['content']['application/json'];

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
