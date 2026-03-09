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
    const suffix = searchParams.size > 0 ? `?${searchParams.toString()}` : '';
    return this.request<ListItpEventsResult>('GET', `/api/itp/events${suffix}`);
  }
}
