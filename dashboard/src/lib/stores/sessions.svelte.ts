import { getGhostClient } from '$lib/ghost-client';
import { wsStore } from '$lib/stores/websocket.svelte';
import type { ListRuntimeSessionsResult, RuntimeSession } from '@ghost/sdk';

type SessionFilter = {
  agentId?: string;
};

type LegacyRuntimeSession = {
  session_id: string;
  started_at: string;
  last_event_at: string;
  event_count: number;
  agents?: string;
};

type LegacyRuntimeSessionsResult = {
  sessions: LegacyRuntimeSession[];
  page?: number;
  page_size?: number;
  total: number;
};

type SessionListPayload = ListRuntimeSessionsResult | LegacyRuntimeSessionsResult;

function normalizeSession(session: RuntimeSession | LegacyRuntimeSession): RuntimeSession {
  const legacyAgents = 'agents' in session ? session.agents : undefined;
  const agentIds =
    'agent_ids' in session && Array.isArray(session.agent_ids)
      ? session.agent_ids
      : typeof legacyAgents === 'string' && legacyAgents.length > 0
        ? legacyAgents
            .split(',')
            .map((agent: string) => agent.trim())
            .filter(Boolean)
        : [];

  return {
    ...session,
    agent_ids: agentIds,
    chain_valid: 'chain_valid' in session ? session.chain_valid : true,
    cumulative_cost: 'cumulative_cost' in session ? session.cumulative_cost : 0,
    branched_from: 'branched_from' in session ? session.branched_from : null,
  };
}

function normalizePage(page: SessionListPayload): ListRuntimeSessionsResult {
  if ('data' in page) {
    return page;
  }

  return {
    data: page.sessions.map(normalizeSession),
    next_cursor: null,
    has_more: false,
    total_count: page.total,
  };
}

class SessionsStore {
  list = $state<RuntimeSession[]>([]);
  loading = $state(false);
  loadingMore = $state(false);
  error = $state('');
  hasMore = $state(false);
  nextCursor = $state<string | null>(null);
  totalCount = $state(0);

  private initialized = false;
  private activeFilter: SessionFilter = {};
  private unsubs: Array<() => void> = [];

  async init(filter: SessionFilter = {}) {
    this.activeFilter = filter;
    if (!this.initialized) {
      this.initialized = true;
      this.unsubs.push(
        wsStore.onResync(() => {
          setTimeout(() => {
            void this.refresh();
          }, Math.random() * 1000);
        }),
      );
    }
    await this.refresh();
  }

  async refresh() {
    this.loading = true;
    this.error = '';
    try {
      const page = normalizePage(await this.fetchPage());
      this.list = page.data.map(normalizeSession);
      this.nextCursor = page.next_cursor ?? null;
      this.hasMore = page.has_more;
      this.totalCount = page.total_count;
    } catch (error) {
      this.error = error instanceof Error ? error.message : 'Failed to load sessions';
      this.list = [];
      this.nextCursor = null;
      this.hasMore = false;
      this.totalCount = 0;
    } finally {
      this.loading = false;
    }
  }

  async loadMore() {
    if (this.loadingMore || !this.hasMore || !this.nextCursor) {
      return;
    }

    this.loadingMore = true;
    this.error = '';
    try {
      const page = normalizePage(await this.fetchPage(this.nextCursor));
      this.list = [...this.list, ...page.data.map(normalizeSession)];
      this.nextCursor = page.next_cursor ?? null;
      this.hasMore = page.has_more;
      this.totalCount = page.total_count;
    } catch (error) {
      this.error = error instanceof Error ? error.message : 'Failed to load more sessions';
    } finally {
      this.loadingMore = false;
    }
  }

  destroy() {
    for (const unsub of this.unsubs) {
      unsub();
    }
    this.unsubs = [];
    this.initialized = false;
    this.activeFilter = {};
    this.list = [];
    this.loading = false;
    this.loadingMore = false;
    this.error = '';
    this.hasMore = false;
    this.nextCursor = null;
    this.totalCount = 0;
  }

  private async fetchPage(cursor?: string): Promise<SessionListPayload> {
    const client = await getGhostClient();
    return client.runtimeSessions.list({
      agent_id: this.activeFilter.agentId,
      cursor,
      limit: 50,
    });
  }
}

export const sessionsStore = new SessionsStore();
