/**
 * Sessions store — Svelte 5 runes.
 *
 * REST init from GET /api/sessions.
 *
 * Ref: T-1.8.4, §5.1
 */

import { api } from '$lib/api';
import { wsStore } from '$lib/stores/websocket.svelte';

export interface Session {
  session_id: string;
  agents: string[];
  started_at: string;
  last_event_at: string;
  event_count: number;
}

class SessionsStore {
  list = $state<Session[]>([]);
  loading = $state(false);
  error = $state('');
  private initialized = false;
  private unsubs: Array<() => void> = [];

  get count(): number {
    return this.list.length;
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.loading = true;
    this.error = '';

    try {
      const data = await api.get('/api/sessions');
      this.list = data?.sessions ?? [];
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load sessions';
    }
    this.loading = false;

    // Subscribe to Resync events for full re-fetch on reconnect gap.
    this.unsubs.push(
      wsStore.on('Resync', () => {
        // Stagger to avoid thundering herd on reconnect
        setTimeout(() => this.refresh(), Math.random() * 2000);
      }),
    );
  }

  /** Refresh sessions from REST. */
  async refresh() {
    try {
      const data = await api.get('/api/sessions');
      this.list = data?.sessions ?? [];
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to refresh sessions';
    }
  }

  destroy() {
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
    this.initialized = false;
  }
}

export const sessionsStore = new SessionsStore();
