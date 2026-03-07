/**
 * Audit store — Svelte 5 runes (new).
 *
 * REST query from GET /api/audit with filter params.
 *
 * Ref: T-1.8.5, §5.1
 */

import { api } from '$lib/api';
import { wsStore } from '$lib/stores/websocket.svelte';

export interface AuditEntry {
  id: string;
  timestamp: string;
  event_type: string;
  severity: string;
  details: string;
  agent_id?: string;
  actor_id?: string;
}

export interface AuditFilters {
  page?: number;
  page_size?: number;
  severity?: string;
  event_type?: string;
  agent_id?: string;
  from?: string;
  to?: string;
  search?: string;
}

class AuditStore {
  entries = $state<AuditEntry[]>([]);
  total = $state(0);
  page = $state(1);
  loading = $state(false);
  error = $state('');
  private lastFilters: AuditFilters = {};
  private unsubs: Array<() => void> = [];
  private resyncRegistered = false;

  async query(filters: AuditFilters = {}) {
    this.lastFilters = filters;

    // Register Resync handler on first query.
    if (!this.resyncRegistered) {
      this.resyncRegistered = true;
      this.unsubs.push(
        wsStore.on('Resync', () => {
          // Stagger to avoid thundering herd on reconnect
          setTimeout(() => this.query(this.lastFilters), Math.random() * 2000);
        }),
      );
    }
    this.loading = true;
    this.error = '';

    const params = new URLSearchParams();
    if (filters.page) params.set('page', String(filters.page));
    if (filters.page_size) params.set('page_size', String(filters.page_size));
    if (filters.severity) params.set('severity', filters.severity);
    if (filters.event_type) params.set('event_type', filters.event_type);
    if (filters.agent_id) params.set('agent_id', filters.agent_id);
    if (filters.from) params.set('from', filters.from);
    if (filters.to) params.set('to', filters.to);
    if (filters.search) params.set('search', filters.search);

    const qs = params.toString();
    const path = qs ? `/api/audit?${qs}` : '/api/audit';

    try {
      const data = await api.get(path);
      this.entries = data?.entries ?? [];
      this.total = data?.total ?? this.entries.length;
      this.page = data?.page ?? filters.page ?? 1;
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load audit entries';
    }
    this.loading = false;
  }

  destroy() {
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
    this.resyncRegistered = false;
  }
}

export const auditStore = new AuditStore();
