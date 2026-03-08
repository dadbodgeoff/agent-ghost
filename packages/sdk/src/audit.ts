import type { GhostClientOptions, GhostRequestFn } from './client.js';
import { createTimeoutSignal } from './client.js';
import { GhostAPIError, GhostNetworkError, GhostTimeoutError } from './errors.js';

export interface AuditEntry {
  id: string;
  timestamp: string;
  event_type: string;
  severity: string;
  details: string;
  agent_id?: string;
  actor_id?: string;
}

export interface AuditQueryParams {
  time_start?: string;
  time_end?: string;
  agent_id?: string;
  event_type?: string;
  severity?: string;
  tool_name?: string;
  search?: string;
  page?: number;
  page_size?: number;
}

export interface AuditQueryResult {
  entries: AuditEntry[];
  page: number;
  page_size: number;
  total: number;
  filters_applied?: Record<string, unknown>;
}

export interface AuditExportParams {
  format?: 'json' | 'csv' | 'jsonl';
  agent_id?: string;
  time_start?: string;
  time_end?: string;
}

export class AuditAPI {
  constructor(
    private request: GhostRequestFn,
    private options: GhostClientOptions,
  ) {}

  async query(params?: AuditQueryParams): Promise<AuditQueryResult> {
    const query = new URLSearchParams();

    if (params?.time_start) query.set('time_start', params.time_start);
    if (params?.time_end) query.set('time_end', params.time_end);
    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.event_type) query.set('event_type', params.event_type);
    if (params?.severity) query.set('severity', params.severity);
    if (params?.tool_name) query.set('tool_name', params.tool_name);
    if (params?.search) query.set('search', params.search);
    if (params?.page !== undefined) query.set('page', String(params.page));
    if (params?.page_size !== undefined) query.set('page_size', String(params.page_size));

    const qs = query.toString();
    return this.request<AuditQueryResult>('GET', `/api/audit${qs ? `?${qs}` : ''}`);
  }

  async export(params?: AuditExportParams): Promise<unknown> {
    const query = new URLSearchParams();

    if (params?.format) query.set('format', params.format);
    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.time_start) query.set('time_start', params.time_start);
    if (params?.time_end) query.set('time_end', params.time_end);

    const qs = query.toString();
    return this.request<unknown>('GET', `/api/audit/export${qs ? `?${qs}` : ''}`);
  }

  async exportBlob(params?: AuditExportParams): Promise<Blob> {
    const query = new URLSearchParams();

    if (params?.format) query.set('format', params.format);
    if (params?.agent_id) query.set('agent_id', params.agent_id);
    if (params?.time_start) query.set('time_start', params.time_start);
    if (params?.time_end) query.set('time_end', params.time_end);

    const qs = query.toString();
    const baseUrl = this.options.baseUrl ?? 'http://127.0.0.1:39780';
    const url = `${baseUrl}/api/audit/export${qs ? `?${qs}` : ''}`;
    const fetchFn = this.options.fetch ?? globalThis.fetch;
    const headers: Record<string, string> = {};

    if (this.options.token) {
      headers['Authorization'] = `Bearer ${this.options.token}`;
    }

    let response: Response;
    try {
      response = await fetchFn(url, {
        method: 'GET',
        headers,
        signal: createTimeoutSignal(this.options.timeout),
      });
    } catch (err) {
      if (err instanceof DOMException && err.name === 'TimeoutError') {
        throw new GhostTimeoutError(this.options.timeout!);
      }
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${baseUrl}`,
        err instanceof Error ? err : undefined,
      );
    }

    if (!response.ok) {
      const text = await response.text().catch(() => '');
      throw new GhostAPIError(text || `HTTP ${response.status}`, response.status);
    }

    return response.blob();
  }
}
