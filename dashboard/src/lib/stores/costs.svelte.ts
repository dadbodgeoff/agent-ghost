/**
 * Costs store — Svelte 5 runes (new).
 *
 * SDK-backed cost query state.
 *
 * Ref: T-1.8.6, §5.1
 */

import { getGhostClient } from '$lib/ghost-client';
import { wsStore } from '$lib/stores/websocket.svelte';
import type { AgentCostInfo, KnownWsEvent } from '@ghost/sdk';

type CostUpdateEvent = Extract<KnownWsEvent, { type: 'CostUpdate' }>;

class CostsStore {
  costs = $state<AgentCostInfo[]>([]);
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
    await this.refresh(true);

    this.unsubs.push(
      wsStore.on('CostUpdate', (msg) => {
        this.applyCostUpdate(msg as CostUpdateEvent);
      }),
      wsStore.on('CostDailyReset', () => {
        void this.refresh();
      }),
      wsStore.onResync(() => {
        setTimeout(() => {
          void this.refresh();
        }, Math.random() * 2000);
      }),
    );
  }

  async refresh(withLoading = false) {
    if (withLoading) {
      this.loading = true;
    }
    this.error = '';
    try {
      const client = await getGhostClient();
      this.costs = await client.costs.list();
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to refresh cost data';
    } finally {
      if (withLoading) {
        this.loading = false;
      }
    }
  }

  private applyCostUpdate(update: CostUpdateEvent) {
    const next: AgentCostInfo = {
      agent_id: update.agent_id,
      agent_name: update.agent_name,
      daily_total: update.daily_total,
      compaction_cost: update.compaction_cost,
      spending_cap: update.spending_cap,
      cap_remaining: update.cap_remaining,
      cap_utilization_pct: update.cap_utilization_pct,
    };
    const idx = this.costs.findIndex((cost) => cost.agent_id === update.agent_id);
    if (idx === -1) {
      this.costs = [...this.costs, next];
      return;
    }
    this.costs[idx] = next;
    this.costs = this.costs;
  }

  destroy() {
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
    this.initialized = false;
  }
}

export const costsStore = new CostsStore();
