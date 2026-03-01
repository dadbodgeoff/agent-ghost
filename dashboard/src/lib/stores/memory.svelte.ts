/**
 * Memory store — Svelte 5 runes.
 *
 * REST query from GET /api/memory.
 *
 * Ref: T-1.8.7, §5.1
 */

import { api } from '$lib/api';

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
    } catch (e: any) {
      this.error = e.message || 'Failed to load memories';
    }
    this.loading = false;
  }

  async refresh() {
    try {
      const data = await api.get('/api/memory');
      this.memories = data?.memories ?? [];
    } catch (e: any) {
      this.error = e.message || 'Failed to refresh memories';
    }
  }

  destroy() {
    this.initialized = false;
  }
}

export const memoryStore = new MemoryStore();
