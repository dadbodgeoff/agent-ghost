import type { GhostRequestFn } from './client.js';
import type { components, operations } from './generated-types.js';

// ── Types ──

export type ConvergenceScore = Omit<components['schemas']['ConvergenceScoreResponse'], 'computed_at'> & {
  computed_at: string | null;
};
export type ConvergenceError = components['schemas']['ConvergenceErrorResponse'];
export type ConvergenceScoresResult = Omit<
  components['schemas']['ConvergenceScoresResponse'],
  'scores'
> & {
  scores: ConvergenceScore[];
};
export type ConvergenceHistoryParams = NonNullable<
  operations['get_convergence_history']['parameters']['query']
>;
export type ConvergenceHistoryEntry = Omit<
  components['schemas']['ConvergenceHistoryEntryResponse'],
  'session_id'
> & {
  session_id: string | null;
};
export type ConvergenceHistoryResult = components['schemas']['ConvergenceHistoryResponse'];

// ── API ──

export class ConvergenceAPI {
  constructor(private request: GhostRequestFn) {}

  /** Get convergence scores for all agents. */
  async scores(): Promise<ConvergenceScoresResult> {
    return this.request<ConvergenceScoresResult>('GET', '/api/convergence/scores');
  }

  /** Get persisted convergence history for one agent. */
  async history(agentId: string, params?: ConvergenceHistoryParams): Promise<ConvergenceHistoryResult> {
    const query = new URLSearchParams();

    if (params?.since) query.set('since', params.since);
    if (params?.limit !== undefined) query.set('limit', String(params.limit));

    const qs = query.toString();
    return this.request<ConvergenceHistoryResult>(
      'GET',
      `/api/convergence/history/${encodeURIComponent(agentId)}${qs ? `?${qs}` : ''}`,
    );
  }
}
