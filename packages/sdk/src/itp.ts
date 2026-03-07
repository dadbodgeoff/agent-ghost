import type { GhostRequestFn } from './client.js';

export interface ItpEvent {
  id: string;
  event_type: string;
  platform: string;
  session_id: string;
  content?: string;
  timestamp: string;
  source: string;
}

export interface ListItpEventsParams {
  limit?: number;
}

export interface ListItpEventsResult {
  events: ItpEvent[];
  buffer_count: number;
  extension_connected: boolean;
}

export class ItpAPI {
  constructor(private request: GhostRequestFn) {}

  async list(params: ListItpEventsParams = {}): Promise<ListItpEventsResult> {
    const searchParams = new URLSearchParams();
    if (params.limit !== undefined) {
      searchParams.set('limit', String(params.limit));
    }
    const suffix = searchParams.size > 0 ? `?${searchParams.toString()}` : '';
    return this.request<ListItpEventsResult>('GET', `/api/itp/events${suffix}`);
  }
}
