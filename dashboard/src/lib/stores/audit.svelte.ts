/**
 * Audit store — Svelte 5 runes (new).
 *
 * SDK-backed audit query state.
 *
 * Ref: T-1.8.5, §5.1
 */

import { getGhostClient } from '$lib/ghost-client';
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
        wsStore.onResync(() => {
          // Stagger to avoid thundering herd on reconnect
          setTimeout(() => {
            void this.query(this.lastFilters);
          }, Math.random() * 2000);
        }),
      );
    }
    this.loading = true;
    this.error = '';

    try {
      const client = await getGhostClient();
      const data = await client.audit.query({
        page: filters.page,
        page_size: filters.page_size,
        severity: filters.severity,
        event_type: filters.event_type,
        agent_id: filters.agent_id,
        time_start: filters.from,
        time_end: filters.to,
        search: filters.search,
      });
      this.entries = data.entries ?? [];
      this.total = data.total ?? this.entries.length;
      this.page = data.page ?? filters.page ?? 1;
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
