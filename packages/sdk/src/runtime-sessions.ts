import type { GhostRequestFn, GhostRequestOptions } from './client.js';
import type { components, operations } from './generated-types.js';

export type RuntimeSession = components['schemas']['RuntimeSessionSummary'];
export type SessionEvent = Omit<components['schemas']['SessionEvent'], 'attributes'> & {
  attributes: Record<string, unknown>;
};
export type SessionEventsParams = NonNullable<
  operations['get_session_events']['parameters']['query']
>;
export type SessionEventsResult = Omit<
  components['schemas']['SessionEventsResponse'],
  'events'
> & {
  events: SessionEvent[];
};
export type SessionBookmark = components['schemas']['SessionBookmark'];
export type SessionBookmarksResult =
  operations['list_session_bookmarks']['responses'][200]['content']['application/json'];
export type CreateSessionBookmarkParams =
  operations['create_session_bookmark']['requestBody']['content']['application/json'];
export type CreateSessionBookmarkResult =
  operations['create_session_bookmark']['responses'][201]['content']['application/json'];
export type DeleteSessionBookmarkResult =
  operations['delete_session_bookmark']['responses'][200]['content']['application/json'];
export type BranchSessionParams =
  operations['branch_runtime_session']['requestBody']['content']['application/json'];
export type BranchSessionResult =
  operations['branch_runtime_session']['responses'][201]['content']['application/json'];
export type ListRuntimeSessionsParams = NonNullable<
  operations['list_sessions']['parameters']['query']
>;
export type ListRuntimeSessionsPageResult =
  components['schemas']['RuntimeSessionsPageResponse'];
export type ListRuntimeSessionsCursorResult =
  components['schemas']['RuntimeSessionsCursorResponse'];

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
