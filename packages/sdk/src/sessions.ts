import type { GhostRequestFn, GhostRequestOptions } from './client.js';

// ── Types ──

export interface StudioSession {
  id: string;
  title: string;
  model: string;
  system_prompt: string;
  temperature: number;
  max_tokens: number;
  created_at: string;
  updated_at: string;
}

export interface StudioMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  token_count: number;
  safety_status: 'clean' | 'warning' | 'blocked';
  created_at: string;
}

export interface StudioSessionWithMessages extends StudioSession {
  messages: StudioMessage[];
}

export interface CreateSessionParams {
  title?: string;
  model?: string;
  system_prompt?: string;
  temperature?: number;
  max_tokens?: number;
}

export interface ListSessionsParams {
  limit?: number;
  cursor?: string;
}

export interface RecoverStreamEvent {
  seq: number;
  event_type: string;
  payload: Record<string, unknown>;
  created_at: string;
}

export interface RecoverStreamResult {
  events: RecoverStreamEvent[];
}

export interface ListSessionsResult {
  sessions: StudioSession[];
  next_cursor: string | null;
  has_more: boolean;
}

// ── API ──

export class SessionsAPI {
  constructor(private request: GhostRequestFn) {}

  /** Create a new studio chat session. */
  async create(
    params?: CreateSessionParams,
    options?: GhostRequestOptions,
  ): Promise<StudioSession> {
    return this.request<StudioSession>('POST', '/api/studio/sessions', params ?? {}, options);
  }

  /** List studio chat sessions. */
  async list(params?: ListSessionsParams): Promise<ListSessionsResult> {
    const query = new URLSearchParams();
    if (params?.limit !== undefined) query.set('limit', String(params.limit));
    if (params?.cursor) query.set('cursor', params.cursor);
    const qs = query.toString();
    return this.request<ListSessionsResult>(
      'GET',
      `/api/studio/sessions${qs ? `?${qs}` : ''}`,
    );
  }

  /** Get a session with all its messages. */
  async get(id: string): Promise<StudioSessionWithMessages> {
    return this.request<StudioSessionWithMessages>(
      'GET',
      `/api/studio/sessions/${encodeURIComponent(id)}`,
    );
  }

  /** Delete a studio session. */
  async delete(id: string, options?: GhostRequestOptions): Promise<{ deleted: boolean }> {
    return this.request<{ deleted: boolean }>(
      'DELETE',
      `/api/studio/sessions/${encodeURIComponent(id)}`,
      undefined,
      options,
    );
  }

  async recoverStream(
    id: string,
    params: { message_id: string; after_seq?: number },
  ): Promise<RecoverStreamResult> {
    const query = new URLSearchParams();
    query.set('message_id', params.message_id);
    if (params.after_seq !== undefined) query.set('after_seq', String(params.after_seq));
    const qs = query.toString();
    return this.request<RecoverStreamResult>(
      'GET',
      `/api/studio/sessions/${encodeURIComponent(id)}/stream/recover?${qs}`,
    );
  }
}
