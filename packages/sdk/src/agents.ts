import type { GhostRequestFn } from './client.js';

// ── Types ──

export interface Agent {
  id: string;
  name: string;
  status: 'Starting' | 'Running' | 'Stopping' | 'Stopped';
  spending_cap: number;
}

export interface AgentDetail extends Agent {
  has_keypair?: boolean;
}

export interface CreateAgentParams {
  name: string;
  spending_cap?: number;
  capabilities?: string[];
  generate_keypair?: boolean;
}

export interface DeleteAgentResult {
  status: 'deleted';
  id: string;
  name: string;
}

// ── API ──

export class AgentsAPI {
  constructor(private request: GhostRequestFn) {}

  /** List all registered agents. */
  async list(): Promise<Agent[]> {
    return this.request<Agent[]>('GET', '/api/agents');
  }

  /** Create a new agent with optional keypair generation. */
  async create(params: CreateAgentParams): Promise<AgentDetail> {
    return this.request<AgentDetail>('POST', '/api/agents', params);
  }

  /** Delete an agent by ID or name. */
  async delete(id: string): Promise<DeleteAgentResult> {
    return this.request<DeleteAgentResult>('DELETE', `/api/agents/${encodeURIComponent(id)}`);
  }
}
