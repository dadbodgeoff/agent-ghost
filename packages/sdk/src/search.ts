import type { GhostRequestFn } from './client.js';

export interface SearchParams {
  q: string;
  types?: string;
  limit?: number;
}

export interface SearchResult {
  result_type: string;
  id: string;
  title: string;
  snippet: string;
  score: number;
}

export interface SearchResponse {
  query: string;
  results: SearchResult[];
  total: number;
}

export class SearchAPI {
  constructor(private request: GhostRequestFn) {}

  async query(params: SearchParams): Promise<SearchResponse> {
    const query = new URLSearchParams();
    query.set('q', params.q);
    if (params.types) query.set('types', params.types);
    if (params.limit !== undefined) query.set('limit', String(params.limit));

    return this.request<SearchResponse>('GET', `/api/search?${query.toString()}`);
  }
}
