import type { GhostRequestFn } from './client.js';

export interface IntegrityBreak {
  session_id?: string;
  memory_id?: string;
  event_id: string | number;
  position: number;
  expected_prev: string;
  actual_prev: string;
}

export interface ItpEventsIntegrity {
  sessions_checked: number;
  total_events: number;
  verified_events: number;
  is_valid: boolean;
  breaks: IntegrityBreak[];
}

export interface MemoryEventsIntegrity {
  memory_chains_checked: number;
  total_events: number;
  verified_events: number;
  is_valid: boolean;
  breaks: IntegrityBreak[];
}

export interface IntegrityChains {
  itp_events?: ItpEventsIntegrity;
  memory_events?: MemoryEventsIntegrity;
}

export interface VerifyChainParams {
  chain?: 'itp' | 'memory' | 'both';
}

export interface VerifyChainResult {
  agent_id: string;
  chain_type: string;
  chains: IntegrityChains;
}

export class IntegrityAPI {
  constructor(private request: GhostRequestFn) {}

  async verifyChain(agentId: string, params?: VerifyChainParams): Promise<VerifyChainResult> {
    const query = new URLSearchParams();

    if (params?.chain) query.set('chain', params.chain);

    const qs = query.toString();
    return this.request<VerifyChainResult>(
      'GET',
      `/api/integrity/chain/${encodeURIComponent(agentId)}${qs ? `?${qs}` : ''}`,
    );
  }
}
