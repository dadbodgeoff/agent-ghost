/**
 * Memory store — Svelte 5 runes.
 *
 * REST query from GET /api/memory.
 *
 * Ref: T-1.8.7, §5.1
 */

import { api } from '$lib/api';
import { wsStore } from '$lib/stores/websocket.svelte';

export interface Memory {
  memory_id: string;
  snapshot: string;
  created_at: string;
  agent_id?: string;
}

class MemoryStore {
  memories = $state<Memory[]>([]);
  loading = $state(false);
  error = $state('');
  private initialized = false;
  private unsubs: Array<() => void> = [];

  get count(): number {
    return this.memories.length;
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.loading = true;
    this.error = '';

    try {
      const data = await api.get('/api/memory');
      this.memories = data?.memories ?? [];
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load memories';
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

  async refresh() {
    try {
      const data = await api.get('/api/memory');
      this.memories = data?.memories ?? [];
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to refresh memories';
    }
  }

  destroy() {
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
    this.initialized = false;
  }
}

export const memoryStore = new MemoryStore();
