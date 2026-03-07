import type { GhostRequestFn } from './client.js';

export interface TrustNode {
  id: string;
  name: string;
  activity: number;
  convergence_level: number;
}

export interface TrustEdge {
  source: string;
  target: string;
  trust_score: number;
}

export interface TrustGraphResult {
  nodes: TrustNode[];
  edges: TrustEdge[];
}

export interface ConsensusRound {
  proposal_id: string;
  status: string;
  approvals: number;
  rejections: number;
  threshold: number;
}

export interface ConsensusResult {
  rounds: ConsensusRound[];
}

export interface Delegation {
  delegator_id: string;
  delegate_id: string;
  scope: string;
  state: string;
  created_at: string;
}

export interface SybilMetrics {
  total_delegations: number;
  max_chain_depth: number;
  unique_delegators: number;
}

export interface DelegationsResult {
  delegations: Delegation[];
  sybil_metrics: SybilMetrics;
}

export class MeshAPI {
  constructor(private request: GhostRequestFn) {}

  async trustGraph(): Promise<TrustGraphResult> {
    return this.request<TrustGraphResult>('GET', '/api/mesh/trust-graph');
  }

  async consensus(): Promise<ConsensusResult> {
    return this.request<ConsensusResult>('GET', '/api/mesh/consensus');
  }

  async delegations(): Promise<DelegationsResult> {
    return this.request<DelegationsResult>('GET', '/api/mesh/delegations');
  }
}
