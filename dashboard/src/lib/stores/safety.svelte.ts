/**
 * Safety store — Svelte 5 runes (new).
 *
 * REST init from GET /api/safety/status.
 * Subscribes to KillSwitchActivation + InterventionChange WS events.
 *
 * Ref: T-1.8.3, §5.1
 */

import { wsStore, type WsMessage } from './websocket.svelte';
import { api } from '$lib/api';

export interface SafetyStatus {
  platform_level: number;
  per_agent: Record<string, { level: number; reason?: string }>;
  gate_status?: Record<string, boolean>;
}

class SafetyStore {
  status = $state<SafetyStatus | null>(null);
  loading = $state(false);
  error = $state('');
  private initialized = false;
  private unsubs: (() => void)[] = [];

  get platformLevel(): number {
    return this.status?.platform_level ?? 0;
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.loading = true;
    this.error = '';

    try {
      const data = await api.get('/api/safety/status');
      this.status = data;
    } catch (e: any) {
      this.error = e.message || 'Failed to load safety status';
    }
    this.loading = false;

    this.unsubs.push(
      wsStore.on('KillSwitchActivation', (msg: WsMessage) => {
        const level = msg.level as number;
        if (this.status) {
          this.status = { ...this.status, platform_level: level };
        }
      }),
      wsStore.on('InterventionChange', (msg: WsMessage) => {
        const agentId = msg.agent_id as string;
        const level = msg.level as number;
        if (this.status && agentId) {
          const perAgent = { ...this.status.per_agent };
          perAgent[agentId] = { level, reason: msg.reason as string | undefined };
          this.status = { ...this.status, per_agent: perAgent };
        }
      }),
    );
  }

  /** Refresh safety status from REST. */
  async refresh() {
    try {
      this.status = await api.get('/api/safety/status');
    } catch (e: any) {
      this.error = e.message || 'Failed to refresh safety status';
    }
  }

  destroy() {
    this.unsubs.forEach(fn => fn());
    this.unsubs = [];
    this.initialized = false;
  }
}

export const safetyStore = new SafetyStore();
