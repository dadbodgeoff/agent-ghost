import type { GhostRequestFn } from './client.js';

export interface MemoryEntry {
  id?: number;
  memory_id: string;
  snapshot: string;
  created_at: string;
}

export interface ListMemoriesParams {
  agent_id?: string;
  page?: number;
  page_size?: number;
  include_archived?: boolean;
}

export interface ListMemoriesResult {
  memories: MemoryEntry[];
  page: number;
  page_size: number;
  total: number;
}

export interface SearchMemoriesParams {
  q?: string;
  agent_id?: string;
  memory_type?: string;
  importance?: string;
  confidence_min?: number;
  confidence_max?: number;
  limit?: number;
  include_archived?: boolean;
}

export interface MemorySearchResultEntry {
  id: number;
  memory_id: string;
  snapshot: unknown;
  created_at: string;
  score: number;
}

export interface SearchMemoriesResult {
  results: MemorySearchResultEntry[];
  count: number;
  query?: string;
  search_mode: 'fts5' | 'like';
  filters: {
    agent_id?: string;
    memory_type?: string;
    importance?: string;
    confidence_min?: number;
    confidence_max?: number;
  };
}

export interface MemoryGraphNode {
  id: string;
  label: string;
  type: 'entity' | 'event' | 'concept';
  importance: number;
  decayFactor: number;
}

export interface MemoryGraphEdge {
  source: string | MemoryGraphNode;
  target: string | MemoryGraphNode;
  relationship: string;
  strength: number;
}

export interface MemoryGraphResult {
  nodes: MemoryGraphNode[];
  edges: MemoryGraphEdge[];
}

export class MemoryAPI {
  constructor(private request: GhostRequestFn) {}

  async list(params?: ListMemoriesParams): Promise<ListMemoriesResult> {
    const query = new URLSearchParams();

    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.page !== undefined) query.set('page', String(params.page));
    if (params?.page_size !== undefined) query.set('page_size', String(params.page_size));
    if (params?.include_archived !== undefined) {
      query.set('include_archived', String(params.include_archived));
    }

    const qs = query.toString();
    return this.request<ListMemoriesResult>('GET', `/api/memory${qs ? `?${qs}` : ''}`);
  }

  async get(id: string): Promise<MemoryEntry> {
    return this.request<MemoryEntry>('GET', `/api/memory/${encodeURIComponent(id)}`);
  }

  async graph(): Promise<MemoryGraphResult> {
    return this.request<MemoryGraphResult>('GET', '/api/memory/graph');
  }

  async search(params?: SearchMemoriesParams): Promise<SearchMemoriesResult> {
    const query = new URLSearchParams();

    if (params?.q) query.set('q', params.q);
    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.memory_type) query.set('memory_type', params.memory_type);
    if (params?.importance) query.set('importance', params.importance);
    if (params?.confidence_min !== undefined) query.set('confidence_min', String(params.confidence_min));
    if (params?.confidence_max !== undefined) query.set('confidence_max', String(params.confidence_max));
    if (params?.limit !== undefined) query.set('limit', String(params.limit));
    if (params?.include_archived !== undefined) {
      query.set('include_archived', String(params.include_archived));
    }

    const qs = query.toString();
    return this.request<SearchMemoriesResult>('GET', `/api/memory/search${qs ? `?${qs}` : ''}`);
  }
}
