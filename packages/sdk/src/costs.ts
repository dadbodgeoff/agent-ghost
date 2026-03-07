import type { GhostRequestFn } from './client.js';

export interface AgentCostInfo {
  agent_id: string;
  agent_name: string;
  daily_total: number;
  compaction_cost: number;
  spending_cap: number;
  cap_remaining: number;
  cap_utilization_pct: number;
}

export class CostsAPI {
  constructor(private request: GhostRequestFn) {}

  async list(): Promise<AgentCostInfo[]> {
    return this.request<AgentCostInfo[]>('GET', '/api/costs');
  }
}
