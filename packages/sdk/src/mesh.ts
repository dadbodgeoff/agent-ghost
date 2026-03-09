import type { GhostRequestFn } from './client.js';
import type { components, operations } from './generated-types.js';

export type TrustNode = components['schemas']['TrustNode'];
export type TrustEdge = components['schemas']['TrustEdge'];
export type TrustGraphResult =
  operations['get_mesh_trust_graph']['responses'][200]['content']['application/json'];
export type ConsensusRound = components['schemas']['ConsensusRound'];
export type ConsensusResult =
  operations['get_mesh_consensus']['responses'][200]['content']['application/json'];
export type Delegation = components['schemas']['Delegation'];
export type SybilMetrics = components['schemas']['SybilMetrics'];
export type DelegationsResult =
  operations['list_mesh_delegations']['responses'][200]['content']['application/json'];

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
