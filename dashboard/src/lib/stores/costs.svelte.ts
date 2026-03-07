/**
 * Costs store — Svelte 5 runes (new).
 *
 * REST query from GET /api/costs.
 *
 * Ref: T-1.8.6, §5.1
 */

import { api } from '$lib/api';
import { wsStore } from '$lib/stores/websocket.svelte';

export interface AgentCost {
  agent_id: string;
  agent_name: string;
  daily_total: number;
  compaction_cost: number;
  spending_cap: number;
}

class CostsStore {
  costs = $state<AgentCost[]>([]);
  loading = $state(false);
  error = $state('');
  private initialized = false;
  private unsubs: Array<() => void> = [];

  /** Total daily spend across all agents. */
  get totalDailySpend(): number {
    return this.costs.reduce((sum, c) => sum + c.daily_total, 0);
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.loading = true;
    this.error = '';

    try {
      const data = await api.get('/api/costs');
      this.costs = Array.isArray(data) ? data : (data?.costs ?? []);
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load cost data';
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
      const data = await api.get('/api/costs');
      this.costs = Array.isArray(data) ? data : (data?.costs ?? []);
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to refresh cost data';
    }
  }

  destroy() {
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
    this.initialized = false;
  }
}

export const costsStore = new CostsStore();
