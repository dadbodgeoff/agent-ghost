/**
 * Safety store — Svelte 5 runes (new).
 *
 * REST init from GET /api/safety/status.
 * Subscribes to KillSwitchActivation + InterventionChange WS events.
 *
 * Ref: T-1.8.3, §5.1
 */

import { wsStore, type WsMessage } from './websocket.svelte';
import { getGhostClient } from '$lib/ghost-client';

export interface SafetyStatus {
  platform_level: number;
  per_agent: Record<string, { level: number; reason?: string }>;
  gate_status?: Record<string, boolean>;
}

function normalizeLevel(level: string | number | undefined): number {
  if (typeof level === 'number') return level;
  if (!level) return 0;

  const parsed = Number(level);
  if (Number.isFinite(parsed)) return parsed;

  switch (level.toUpperCase()) {
    case 'SOFT':
    case 'L1':
      return 1;
    case 'ACTIVE':
    case 'L2':
      return 2;
    case 'HARD':
    case 'L3':
      return 3;
    case 'EXTERNAL':
    case 'L4':
      return 4;
    default:
      return 0;
  }
}

async function loadSafetyStatus(): Promise<SafetyStatus> {
  const client = await getGhostClient();
  const data = await client.safety.status();

  const perAgent = Object.fromEntries(
    Object.entries(data.per_agent ?? {}).map(([agentId, value]) => [
      agentId,
      {
        level: normalizeLevel(value.level),
        reason: value.trigger,
      },
    ]),
  );

  return {
    platform_level: normalizeLevel(data.platform_level),
    per_agent: perAgent,
  };
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
      this.status = await loadSafetyStatus();
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load safety status';
    }
    this.loading = false;

    this.unsubs.push(
      wsStore.on('KillSwitchActivation', (msg: WsMessage) => {
        const level = normalizeLevel(msg.level as string | number | undefined);
        if (this.status) {
          this.status = { ...this.status, platform_level: level };
        }
      }),
      wsStore.on('InterventionChange', (msg: WsMessage) => {
        const agentId = msg.agent_id as string;
        const level = normalizeLevel(
          (msg as { level?: number; new_level?: number }).new_level ?? msg.level as number | undefined,
        );
        if (this.status && agentId) {
          const perAgent = { ...this.status.per_agent };
          perAgent[agentId] = { level, reason: msg.reason as string | undefined };
          this.status = { ...this.status, per_agent: perAgent };
        }
      }),
      wsStore.on('Resync', () => {
        // Stagger to avoid thundering herd on reconnect
        setTimeout(() => this.refresh(), Math.random() * 2000);
      }),
    );
  }

  /** Refresh safety status from REST. */
  async refresh() {
    try {
      this.status = await loadSafetyStatus();
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to refresh safety status';
    }
  }

  destroy() {
    this.unsubs.forEach(fn => fn());
    this.unsubs = [];
    this.initialized = false;
  }
}

export const safetyStore = new SafetyStore();
