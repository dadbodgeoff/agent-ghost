import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

export type StudioRunMessageInput = Omit<components['schemas']['StudioMessage'], 'role'> & {
  role: 'user' | 'assistant' | 'system';
};

export type StudioRunParams = Omit<
  operations['studio_run']['requestBody']['content']['application/json'],
  'messages'
> & {
  messages: StudioRunMessageInput[];
};

export type StudioRunResult = components['schemas']['StudioRunResponse'];

export class StudioAPI {
  constructor(private request: GhostRequestFn) {}

  async run(params: StudioRunParams, options?: GhostRequestOptions): Promise<StudioRunResult> {
    return this.request<StudioRunResult>('POST', '/api/studio/run', params, options);
  }
}
