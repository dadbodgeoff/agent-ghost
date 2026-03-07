import type { GhostRequestFn } from './client.js';

export interface ChannelInfo {
  id: string;
  channel_type: string;
  status: string;
  status_message?: string | null;
  agent_id: string;
  agent_name?: string;
  config: Record<string, unknown>;
  last_message_at: string | null;
  message_count: number;
}

export interface ListChannelsResult {
  channels: ChannelInfo[];
}

export interface CreateChannelParams {
  channel_type: string;
  agent_id: string;
  config?: Record<string, unknown>;
}

export interface CreateChannelResult {
  id: string;
  status: 'created';
}

export interface ReconnectChannelResult {
  id: string;
  status: 'reconnected';
}

export interface DeleteChannelResult {
  id: string;
  status: 'deleted';
}

export class ChannelsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListChannelsResult> {
    return this.request<ListChannelsResult>('GET', '/api/channels');
  }

  async create(params: CreateChannelParams): Promise<CreateChannelResult> {
    return this.request<CreateChannelResult>('POST', '/api/channels', params);
  }

  async reconnect(id: string): Promise<ReconnectChannelResult> {
    return this.request<ReconnectChannelResult>(
      'POST',
      `/api/channels/${encodeURIComponent(id)}/reconnect`,
      {},
    );
  }

  async delete(id: string): Promise<DeleteChannelResult> {
    return this.request<DeleteChannelResult>('DELETE', `/api/channels/${encodeURIComponent(id)}`);
  }
}
