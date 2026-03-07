import type { GhostRequestFn } from './client.js';

export interface TraceSpanRecord {
  span_id: string;
  trace_id: string;
  parent_span_id: string | null;
  operation_name: string;
  start_time: string;
  end_time: string | null;
  attributes: Record<string, unknown>;
  status: string;
}

export interface TraceGroup {
  trace_id: string;
  spans: TraceSpanRecord[];
}

export interface SessionTrace {
  session_id: string;
  traces: TraceGroup[];
  total_spans: number;
}

export class TracesAPI {
  constructor(private request: GhostRequestFn) {}

  async get(sessionId: string): Promise<SessionTrace> {
    return this.request<SessionTrace>('GET', `/api/traces/${encodeURIComponent(sessionId)}`);
  }
}
