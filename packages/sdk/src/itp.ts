import type { GhostRequestFn } from './client.js';
import type { components, operations } from './generated-types.js';

export type ItpEvent = components['schemas']['ItpEvent'];
export type ListItpEventsParams = NonNullable<
  operations['list_itp_events']['parameters']['query']
>;
export type ListItpEventsResult =
  operations['list_itp_events']['responses'][200]['content']['application/json'];

export class ItpAPI {
  constructor(private request: GhostRequestFn) {}

  async list(params: ListItpEventsParams = {}): Promise<ListItpEventsResult> {
    const searchParams = new URLSearchParams();
    if (params.limit !== undefined) {
      searchParams.set('limit', String(params.limit));
    }
    if (params.offset !== undefined) {
      searchParams.set('offset', String(params.offset));
    }
    if (params.session_id) {
      searchParams.set('session_id', params.session_id);
    }
    if (params.event_type) {
      searchParams.set('event_type', params.event_type);
    }
    const suffix = searchParams.size > 0 ? `?${searchParams.toString()}` : '';
    return this.request<ListItpEventsResult>('GET', `/api/itp/events${suffix}`);
  }
}
