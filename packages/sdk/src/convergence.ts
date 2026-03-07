import type { GhostRequestFn } from './client.js';

// ── Types ──

export interface ConvergenceScore {
  agent_id: string;
  agent_name: string;
  score: number;
  level: number;
  profile: string;
  signal_scores: Record<string, number>;
  computed_at: string;
}

export interface ConvergenceError {
  agent_id: string;
  agent_name: string;
  error: string;
}

export interface ConvergenceScoresResult {
  scores: ConvergenceScore[];
  errors?: ConvergenceError[];
}

// ── API ──

export class ConvergenceAPI {
  constructor(private request: GhostRequestFn) {}

  /** Get convergence scores for all agents. */
  async scores(): Promise<ConvergenceScoresResult> {
    return this.request<ConvergenceScoresResult>('GET', '/api/convergence/scores');
  }
}
