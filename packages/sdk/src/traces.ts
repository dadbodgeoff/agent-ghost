import type { GhostRequestFn } from './client.js';
import type { components } from './generated-types.js';

export type TraceSpanRecord = Omit<
  components['schemas']['SpanRecord'],
  'end_time' | 'parent_span_id'
> & {
  end_time: string | null;
  parent_span_id: string | null;
};
export type TraceGroup = Omit<components['schemas']['TraceGroup'], 'spans'> & {
  spans: TraceSpanRecord[];
};
export type SessionTrace = Omit<components['schemas']['TraceResponse'], 'traces'> & {
  traces: TraceGroup[];
};

export class TracesAPI {
  constructor(private request: GhostRequestFn) {}

  async get(sessionId: string): Promise<SessionTrace> {
    return this.request<SessionTrace>('GET', `/api/traces/${encodeURIComponent(sessionId)}`);
  }
}
