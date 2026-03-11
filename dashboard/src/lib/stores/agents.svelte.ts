/**
 * Agents store — Svelte 5 runes.
 *
 * REST init from GET /api/agents.
 * Subscribes to AgentStateChange WS events.
 *
 * Ref: T-1.8.1, §5.1
 */

import { wsStore, type WsMessage } from './websocket.svelte';
import { getGhostClient } from '$lib/ghost-client';

export interface Agent {
  id: string;
  name: string;
  status: string;
  lifecycle_state?: 'starting' | 'ready' | 'stopping' | 'stopped';
  safety_state?: 'normal' | 'paused' | 'quarantined' | 'kill_all_blocked';
  effective_state?: 'starting' | 'ready' | 'paused' | 'quarantined' | 'kill_all_blocked' | 'stopping' | 'stopped';
  spending_cap?: number;
  capabilities?: string[];
  sandbox?: {
    enabled: boolean;
    mode: 'off' | 'read_only' | 'workspace_write' | 'strict';
    on_violation: 'warn' | 'pause' | 'quarantine' | 'kill_all';
    network_access: boolean;
    allowed_shell_prefixes: string[];
  };
  created_at?: string;
}

class AgentsStore {
  list = $state<Agent[]>([]);
  loading = $state(false);
  error = $state('');
  private initialized = false;
  private unsubs: Array<() => void> = [];

  get count(): number {
    return this.list.length;
  }

  get active(): Agent[] {
    return this.list.filter(a => (a.effective_state ?? a.status) === 'ready');
  }

  /** Fetch agents from REST API and subscribe to WS events. */
  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.loading = true;
    this.error = '';

    try {
      const client = await getGhostClient();
      this.list = await client.agents.list();
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load agents';
    }
    this.loading = false;

    // Subscribe to real-time updates.
    this.unsubs.push(
      wsStore.on('AgentStateChange', (msg: WsMessage) => {
        const agentId = msg.agent_id as string;
        const status = (
          msg.status ??
          (msg as { new_state?: string }).new_state
        ) as string | undefined;
        const idx = this.list.findIndex(a => a.id === agentId);
        if (idx >= 0 && status) {
          this.list[idx] = { ...this.list[idx], status };
          // Trigger reactivity by reassigning.
          this.list = [...this.list];
        }
      }),
      wsStore.on('AgentOperationalStatusChanged', (msg: WsMessage) => {
        const agentId = msg.agent_id as string;
        const idx = this.list.findIndex(a => a.id === agentId);
        if (idx >= 0) {
          this.list[idx] = {
            ...this.list[idx],
            status: (msg.effective_state as string | undefined) ?? this.list[idx].status,
            lifecycle_state: msg.lifecycle_state as Agent['lifecycle_state'],
            safety_state: msg.safety_state as Agent['safety_state'],
            effective_state: msg.effective_state as Agent['effective_state'],
          };
          this.list = [...this.list];
        }
      }),
      wsStore.on('Resync', () => {
        // Stagger to avoid thundering herd on reconnect
        setTimeout(() => this.refresh(), Math.random() * 2000);
      }),
    );
  }

  /** Refresh agents from REST API. */
  async refresh() {
    try {
      const client = await getGhostClient();
      this.list = await client.agents.list();
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to refresh agents';
    }
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
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
    this.initialized = false;
  }
}

export const agentsStore = new AgentsStore();
