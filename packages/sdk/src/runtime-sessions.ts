import type { GhostRequestFn } from './client.js';

export interface RuntimeSession {
  session_id: string;
  started_at: string;
  last_event_at: string;
  event_count: number;
  agents: string[] | string;
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
  offset?: number;
  limit?: number;
}

export interface SessionEventsResult {
  session_id: string;
  events: SessionEvent[];
  total: number;
  offset: number;
  limit: number;
  chain_valid: boolean;
  cumulative_cost: number;
}

export interface SessionBookmark {
  id: string;
  eventIndex: number;
  label: string;
  createdAt: string;
}

export interface SessionBookmarksResult {
  bookmarks: SessionBookmark[];
}

export interface CreateSessionBookmarkParams {
  id?: string;
  eventIndex: number;
  label: string;
}

export interface CreateSessionBookmarkResult {
  id: string;
  status: 'created';
}

export interface DeleteSessionBookmarkResult {
  status: 'deleted';
}

export interface BranchSessionParams {
  from_event_index: number;
}

export interface BranchSessionResult {
  session_id: string;
  branched_from: string;
  events_copied: number;
}

export interface ListRuntimeSessionsParams {
  page?: number;
  page_size?: number;
  cursor?: string;
  limit?: number;
}

export interface ListRuntimeSessionsPageResult {
  sessions: RuntimeSession[];
  page: number;
  page_size: number;
  total: number;
}

export interface ListRuntimeSessionsCursorResult {
  data: RuntimeSession[];
  next_cursor: string | null;
  has_more: boolean;
  total_count: number;
}

export class RuntimeSessionsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(
    params?: ListRuntimeSessionsParams,
  ): Promise<ListRuntimeSessionsPageResult | ListRuntimeSessionsCursorResult> {
    const query = new URLSearchParams();

    if (params?.page !== undefined) query.set('page', String(params.page));
    if (params?.page_size !== undefined) query.set('page_size', String(params.page_size));
    if (params?.cursor) query.set('cursor', params.cursor);
    if (params?.limit !== undefined) query.set('limit', String(params.limit));

    const qs = query.toString();
    return this.request<ListRuntimeSessionsPageResult | ListRuntimeSessionsCursorResult>(
      'GET',
      `/api/sessions${qs ? `?${qs}` : ''}`,
    );
  }

  async events(sessionId: string, params?: SessionEventsParams): Promise<SessionEventsResult> {
    const query = new URLSearchParams();

    if (params?.offset !== undefined) query.set('offset', String(params.offset));
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
  ): Promise<CreateSessionBookmarkResult> {
    return this.request<CreateSessionBookmarkResult>(
      'POST',
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks`,
      params,
    );
  }

  async deleteBookmark(
    sessionId: string,
    bookmarkId: string,
  ): Promise<DeleteSessionBookmarkResult> {
    return this.request<DeleteSessionBookmarkResult>(
      'DELETE',
      `/api/sessions/${encodeURIComponent(sessionId)}/bookmarks/${encodeURIComponent(bookmarkId)}`,
    );
  }

  async branch(sessionId: string, params: BranchSessionParams): Promise<BranchSessionResult> {
    return this.request<BranchSessionResult>(
      'POST',
      `/api/sessions/${encodeURIComponent(sessionId)}/branch`,
      params,
    );
  }

  async heartbeat(sessionId: string): Promise<void> {
    await this.request<void>(
      'POST',
      `/api/sessions/${encodeURIComponent(sessionId)}/heartbeat`,
      {},
    );
  }
}
