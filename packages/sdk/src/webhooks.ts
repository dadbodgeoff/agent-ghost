import type { GhostRequestFn, GhostRequestOptions } from './client.js';

export type WebhookEventType =
  | 'intervention_change'
  | 'kill_switch'
  | 'proposal_decision'
  | 'agent_state_change'
  | 'score_update'
  | 'backup_complete';

export interface WebhookSummary {
  id: string;
  name: string;
  url: string;
  events: WebhookEventType[];
  active: boolean;
  created_at: string;
  updated_at: string;
}

export interface ListWebhooksResult {
  webhooks: WebhookSummary[];
}

export interface CreateWebhookParams {
  name: string;
  url: string;
  secret?: string;
  events: WebhookEventType[];
  headers?: Record<string, string>;
}

export interface UpdateWebhookParams {
  name?: string;
  url?: string;
  events?: WebhookEventType[];
  active?: boolean;
  headers?: Record<string, string>;
}

export interface DeleteWebhookResult {
  deleted: string;
}

export interface TestWebhookResult {
  webhook_id: string;
  status_code: number;
  success: boolean;
}

export class WebhooksAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<ListWebhooksResult> {
    return this.request<ListWebhooksResult>('GET', '/api/webhooks');
  }

  async create(
    params: CreateWebhookParams,
    options?: GhostRequestOptions,
  ): Promise<WebhookSummary> {
    return this.request<WebhookSummary>('POST', '/api/webhooks', params, options);
  }

  async update(
    id: string,
    params: UpdateWebhookParams,
    options?: GhostRequestOptions,
  ): Promise<{ updated: string }> {
    return this.request<{ updated: string }>(
      'PUT',
      `/api/webhooks/${encodeURIComponent(id)}`,
      params,
      options,
    );
  }

  async delete(id: string, options?: GhostRequestOptions): Promise<DeleteWebhookResult> {
    return this.request<DeleteWebhookResult>(
      'DELETE',
      `/api/webhooks/${encodeURIComponent(id)}`,
      undefined,
      options,
    );
  }

  async test(id: string, options?: GhostRequestOptions): Promise<TestWebhookResult> {
    return this.request<TestWebhookResult>(
      'POST',
      `/api/webhooks/${encodeURIComponent(id)}/test`,
      {},
      options,
    );
  }
}
