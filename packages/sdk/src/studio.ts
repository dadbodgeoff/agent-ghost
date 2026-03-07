import type { GhostRequestFn } from './client.js';

export interface StudioRunMessageInput {
  role: 'user' | 'assistant' | 'system';
  content: string;
}

export interface StudioRunParams {
  system_prompt?: string;
  messages: StudioRunMessageInput[];
  model?: string;
  temperature?: number;
  max_tokens?: number;
}

export interface StudioRunResult {
  content: string;
  model: string;
  token_count: number;
  finish_reason: string;
}

export class StudioAPI {
  constructor(private request: GhostRequestFn) {}

  async run(params: StudioRunParams): Promise<StudioRunResult> {
    return this.request<StudioRunResult>('POST', '/api/studio/run', params);
  }
}
