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
  navigation: SearchNavigation;
  match_context: SearchMatchContext;
}

export interface SearchNavigation {
  href: string;
  route_kind: string;
  focus_id?: string;
  query?: string;
}

export interface SearchMatchContext {
  matched_fields: string[];
}

export interface SearchTypeCount {
  result_type: string;
  total: number;
}

export interface SearchDomainWarning {
  result_type: string;
  message: string;
}

export interface SearchResponse {
  query: string;
  results: SearchResult[];
  total: number;
  returned_count: number;
  totals_by_type: SearchTypeCount[];
  degraded: boolean;
  warnings: SearchDomainWarning[];
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
