import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

// ── Types ──

export type StudioSafetyStatus = 'clean' | 'warning' | 'blocked';
export type StudioMessageRole = 'user' | 'assistant' | 'system';

export type StudioSession = components['schemas']['StudioSessionSchema'];
export type StudioMessage = Omit<
  components['schemas']['StudioMessageSchema'],
  'role' | 'safety_status'
> & {
  role: StudioMessageRole;
  safety_status: StudioSafetyStatus;
};
export type StudioSessionWithMessages = Omit<
  components['schemas']['StudioSessionWithMessagesResponseSchema'],
  'messages'
> & {
  messages: StudioMessage[];
};
export type CreateSessionParams =
  operations['create_studio_session']['requestBody']['content']['application/json'];
export type ListSessionsParams = NonNullable<
  operations['list_studio_sessions']['parameters']['query']
>;
export type RecoverStreamEvent = Omit<
  components['schemas']['StudioRecoverStreamEventSchema'],
  'payload'
> & {
  payload: Record<string, unknown>;
};
export type RecoverStreamResult = Omit<
  components['schemas']['StudioRecoverStreamResponseSchema'],
  'events'
> & {
  events: RecoverStreamEvent[];
};
export type ListSessionsResult = Omit<
  components['schemas']['StudioSessionListResponseSchema'],
  'sessions'
> & {
  sessions: StudioSession[];
};

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
