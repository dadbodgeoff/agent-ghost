import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

export interface RuntimeSession {
  session_id: string;
  agent_ids: string[];
  started_at: string;
  last_event_at: string;
  event_count: number;
  chain_valid: boolean;
  cumulative_cost: number;
  branched_from: string | null;
}
export interface RuntimeSessionDetailResult {
  session: RuntimeSession;
  bookmark_count: number;
}
export interface SessionEvent {
  id: string;
  event_type: string;
  sender?: string | null;
  timestamp: string;
  sequence_number: number;
  content_hash?: string | null;
  content_length?: number | null;
  privacy_level: string;
  latency_ms?: number | null;
  token_count?: number | null;
  event_hash: string;
  previous_hash: string;
  attributes: Record<string, unknown>;
}
export interface SessionEventsParams {
  after_sequence_number?: number;
  limit?: number;
}
export interface SessionEventsResult {
  session_id: string;
  events: SessionEvent[];
  total: number;
  limit: number;
  has_more: boolean;
  next_after_sequence_number: number | null;
  chain_valid: boolean;
  cumulative_cost: number;
}
export interface SessionBookmark {
  id: string;
  session_id: string;
  sequence_number: number;
  label: string;
  created_at: string;
}
export interface SessionBookmarksResult {
  bookmarks: SessionBookmark[];
}
export interface CreateSessionBookmarkParams {
  id?: string;
  sequence_number: number;
  label: string;
}
export interface CreateSessionBookmarkResult {
  bookmark: SessionBookmark;
}
export interface DeleteSessionBookmarkResult {
  status: string;
}
export interface BranchSessionParams {
  from_sequence_number: number;
}
export interface BranchSessionResult {
  session: RuntimeSession;
}
export interface ListRuntimeSessionsParams {
  agent_id?: string;
  cursor?: string;
  limit?: number;
}
export interface ListRuntimeSessionsResult {
  data: RuntimeSession[];
  next_cursor: string | null;
  has_more: boolean;
  total_count: number;
}
export type ListRuntimeSessionsPageResult = ListRuntimeSessionsResult;
export type ListRuntimeSessionsCursorResult = ListRuntimeSessionsResult;

export class RuntimeSessionsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(params?: ListRuntimeSessionsParams): Promise<ListRuntimeSessionsResult> {
    const query = new URLSearchParams();

    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.cursor) query.set('cursor', params.cursor);
    if (params?.limit !== undefined) query.set('limit', String(params.limit));

    const qs = query.toString();
    return this.request<ListRuntimeSessionsResult>('GET', `/api/sessions${qs ? `?${qs}` : ''}`);
  }

  async get(sessionId: string): Promise<RuntimeSessionDetailResult> {
    return this.request<RuntimeSessionDetailResult>(
      'GET',
      `/api/sessions/${encodeURIComponent(sessionId)}`,
    );
  }

  async events(sessionId: string, params?: SessionEventsParams): Promise<SessionEventsResult> {
    const query = new URLSearchParams();

    if (params?.after_sequence_number !== undefined) {
      query.set('after_sequence_number', String(params.after_sequence_number));
    }
    if (params?.limit !== undefined) query.set('limit', String(params.limit));

    const qs = query.toString();
    return this.request<SessionEventsResult>(
      'GET',
      `/api/sessions/${encodeURIComponent(sessionId)}/events${qs ? `?${qs}` : ''}`,
    );
  }

  async listBookmarks(sessionId: string): Promise<SessionBookmarksResult> {
    return this.request<SessionBookmarksResult>(
      'GET',
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks`,
    );
  }

  async createBookmark(
    sessionId: string,
    params: CreateSessionBookmarkParams,
    options?: GhostRequestOptions,
  ): Promise<CreateSessionBookmarkResult> {
    return this.request<CreateSessionBookmarkResult>(
      'POST',
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks`,
      params,
      options,
    );
  }

  async deleteBookmark(
    sessionId: string,
    bookmarkId: string,
    options?: GhostRequestOptions,
  ): Promise<DeleteSessionBookmarkResult> {
    return this.request<DeleteSessionBookmarkResult>(
      'DELETE',
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks/${encodeURIComponent(bookmarkId)}`,
      undefined,
      options,
    );
  }

  async branch(
    sessionId: string,
    params: BranchSessionParams,
    options?: GhostRequestOptions,
  ): Promise<BranchSessionResult> {
    return this.request<BranchSessionResult>(
      'POST',
      `/api/sessions/${encodeURIComponent(sessionId)}/branch`,
      params,
      options,
    );
  }

  async heartbeat(sessionId: string, options?: GhostRequestOptions): Promise<void> {
    await this.request<void>(
      'POST',
      `/api/sessions/${encodeURIComponent(sessionId)}/heartbeat`,
      {},
      options,
    );
  }
}
