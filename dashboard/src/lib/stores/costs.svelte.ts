/**
 * Costs store — Svelte 5 runes (new).
 *
 * REST query from GET /api/costs.
 *
 * Ref: T-1.8.6, §5.1
 */

import { api } from '$lib/api';

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
    } catch (e: any) {
      this.error = e.message || 'Failed to load cost data';
    }
    this.loading = false;
  }

  async refresh() {
    try {
      const data = await api.get('/api/costs');
      this.costs = Array.isArray(data) ? data : (data?.costs ?? []);
    } catch (e: any) {
      this.error = e.message || 'Failed to refresh cost data';
    }
  }

  destroy() {
    this.initialized = false;
  }
}

export const costsStore = new CostsStore();
