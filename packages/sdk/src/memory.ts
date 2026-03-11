import type { GhostRequestFn } from './client.js';
import type { components, operations } from './generated-types.js';

export type MemoryEntry = components['schemas']['MemoryEntry'];
export type ListMemoriesParams = NonNullable<operations['list_memories']['parameters']['query']>;
export type ListMemoriesResult =
  operations['list_memories']['responses'][200]['content']['application/json'];
export type SearchMemoriesParams = NonNullable<
  operations['search_memories']['parameters']['query']
>;
export type MemorySearchResultEntry = components['schemas']['MemorySearchResultEntry'];
export type SearchMemoriesResult =
  operations['search_memories']['responses'][200]['content']['application/json'];
export type ArchiveMemoryParams = components['schemas']['ArchiveMemoryRequest'];
export type MemoryArchiveStatus = components['schemas']['MemoryArchiveStatusResponse'];
export type MemoryGraphNode = components['schemas']['MemoryGraphNode'];
export type MemoryGraphEdge = Omit<components['schemas']['MemoryGraphEdge'], 'source' | 'target'> & {
  source: string | MemoryGraphNode;
  target: string | MemoryGraphNode;
};
export type MemoryGraphResult = Omit<
  operations['get_memory_graph']['responses'][200]['content']['application/json'],
  'edges'
> & {
  edges: MemoryGraphEdge[];
};

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

  async archive(id: string, body: ArchiveMemoryParams): Promise<MemoryArchiveStatus> {
    return this.request<MemoryArchiveStatus>(
      'POST',
      `/api/memory/${encodeURIComponent(id)}/archive`,
      body,
    );
  }

  async unarchive(id: string): Promise<MemoryArchiveStatus> {
    return this.request<MemoryArchiveStatus>(
      'POST',
      `/api/memory/${encodeURIComponent(id)}/unarchive`,
    );
  }
}
