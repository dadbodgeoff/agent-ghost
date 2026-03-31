/**
 * Convergence store — Svelte 5 runes.
 *
 * REST init from GET /api/convergence/scores.
 * Subscribes to ScoreUpdate + InterventionChange WS events.
 *
 * Ref: T-1.8.2, §5.1
 */

import { wsStore, type WsMessage } from './websocket.svelte';
import { getGhostClient } from '$lib/ghost-client';

export interface AgentScore {
  agent_id: string;
  agent_name: string;
  score: number;
  level: number;
  profile: string;
  signal_scores: Record<string, number>;
  computed_at: string | null;
}

class ConvergenceStore {
  scores = $state<AgentScore[]>([]);
  loading = $state(false);
  error = $state('');
  monitorOnline = $state(true);
  private initialized = false;
  private unsubs: (() => void)[] = [];

  /** Composite score of the first agent (overview shortcut). */
  get primaryScore(): number {
    return this.scores[0]?.score ?? 0;
  }

  get primaryLevel(): number {
    return this.scores[0]?.level ?? 0;
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    this.loading = true;
    this.error = '';

    try {
      const client = await getGhostClient();
      const data = await client.convergence.scores();
      this.scores = data.scores ?? [];
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load convergence data';
    }
    this.loading = false;

    // Subscribe to real-time score updates.
    this.unsubs.push(
      wsStore.on('ScoreUpdate', (msg: WsMessage) => {
        const agentId = msg.agent_id as string;
        const newScore = msg.score as number;
        const newLevel = msg.level as number | undefined;

        const idx = this.scores.findIndex(s => s.agent_id === agentId);
        if (idx >= 0) {
          this.scores[idx] = {
            ...this.scores[idx],
            score: newScore ?? this.scores[idx].score,
            level: newLevel ?? this.scores[idx].level,
            computed_at: new Date().toISOString(),
          };
          this.scores = [...this.scores];
        }
      }),
      wsStore.on('InterventionChange', (msg: WsMessage) => {
        const agentId = msg.agent_id as string;
        const newLevel = msg.new_level as number;
        const idx = this.scores.findIndex(s => s.agent_id === agentId);
        if (idx >= 0) {
          this.scores[idx] = { ...this.scores[idx], level: newLevel };
          this.scores = [...this.scores];
        }
      }),
      wsStore.on('Resync', () => {
        // Stagger to avoid thundering herd on reconnect
        setTimeout(() => this.refresh(), Math.random() * 2000);
      }),
    );
  }

  /** Refresh convergence scores from REST. */
  async refresh() {
    this.error = '';
    try {
      const client = await getGhostClient();
      const data = await client.convergence.scores();
      this.scores = data.scores ?? [];
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to refresh convergence data';
    }
  }

  /** Poll monitor health (called periodically from layout). */
  async checkMonitorHealth() {
    try {
      const client = await getGhostClient();
      await client.health.check();
      this.monitorOnline = true;
    } catch {
      this.monitorOnline = false;
    }
  }

  destroy() {
    this.unsubs.forEach(fn => fn());
    this.unsubs = [];
    this.initialized = false;
  }
}

export const convergenceStore = new ConvergenceStore();
