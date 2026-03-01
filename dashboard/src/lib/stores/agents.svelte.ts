/**
 * Agents store — Svelte 5 runes.
 *
 * REST init from GET /api/agents.
 * Subscribes to AgentStateChange WS events.
 *
 * Ref: T-1.8.1, §5.1
 */

import { wsStore, type WsMessage } from './websocket.svelte';
import { api } from '$lib/api';

export interface Agent {
  id: string;
  name: string;
  status: string;
  spending_cap?: number;
  capabilities?: string[];
  created_at?: string;
}

class AgentsStore {
  list = $state<Agent[]>([]);
  loading = $state(false);
  error = $state('');
  private initialized = false;
  private unsubscribe: (() => void) | null = null;

  get count(): number {
    return this.list.length;
  }

  get active(): Agent[] {
    return this.list.filter(a => a.status === 'active');
  }

  /** Fetch agents from REST API and subscribe to WS events. */
  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.loading = true;
    this.error = '';

    try {
      const data = await api.get('/api/agents');
      this.list = Array.isArray(data) ? data : [];
    } catch (e: any) {
      this.error = e.message || 'Failed to load agents';
    }
    this.loading = false;

    // Subscribe to real-time updates.
    this.unsubscribe = wsStore.on('AgentStateChange', (msg: WsMessage) => {
      const agentId = msg.agent_id as string;
      const status = msg.status as string | undefined;
      const idx = this.list.findIndex(a => a.id === agentId);
      if (idx >= 0 && status) {
        this.list[idx] = { ...this.list[idx], status };
        // Trigger reactivity by reassigning.
        this.list = [...this.list];
      }
    });
  }

  /** Add a newly created agent to the local list. */
  add(agent: Agent) {
    this.list = [...this.list, agent];
  }

  /** Remove an agent by ID (soft-delete: mark as deleted). */
  remove(id: string) {
    this.list = this.list.map(a =>
      a.id === id ? { ...a, status: 'deleted' } : a
    );
  }

  destroy() {
    this.unsubscribe?.();
    this.unsubscribe = null;
    this.initialized = false;
  }
}

export const agentsStore = new AgentsStore();
