import type { GhostRequestFn } from './client.js';

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
  offset?: number;
}

// ── API ──

export class SessionsAPI {
  constructor(private request: GhostRequestFn) {}

  /** Create a new studio chat session. */
  async create(params?: CreateSessionParams): Promise<StudioSession> {
    return this.request<StudioSession>('POST', '/api/studio/sessions', params ?? {});
  }

  /** List studio chat sessions. */
  async list(params?: ListSessionsParams): Promise<{ sessions: StudioSession[] }> {
    const query = new URLSearchParams();
    if (params?.limit !== undefined) query.set('limit', String(params.limit));
    if (params?.offset !== undefined) query.set('offset', String(params.offset));
    const qs = query.toString();
    return this.request<{ sessions: StudioSession[] }>(
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
  async delete(id: string): Promise<{ deleted: boolean }> {
    return this.request<{ deleted: boolean }>(
      'DELETE',
      `/api/studio/sessions/${encodeURIComponent(id)}`,
    );
  }
}
