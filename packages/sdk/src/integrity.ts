import type { GhostRequestFn } from './client.js';
import type { components, operations } from './generated-types.js';

export type IntegrityBreak = Omit<components['schemas']['IntegrityBreak'], 'event_id'> & {
  event_id: string | number;
};
export type ItpEventsIntegrity = Omit<components['schemas']['ItpEventsIntegrity'], 'breaks'> & {
  breaks: IntegrityBreak[];
};
export type MemoryEventsIntegrity = Omit<
  components['schemas']['MemoryEventsIntegrity'],
  'breaks'
> & {
  breaks: IntegrityBreak[];
};
export type IntegrityChains = Omit<
  components['schemas']['IntegrityChains'],
  'itp_events' | 'memory_events'
> & {
  itp_events?: ItpEventsIntegrity | null;
  memory_events?: MemoryEventsIntegrity | null;
};
export type VerifyChainParams = NonNullable<
  operations['verify_integrity_chain']['parameters']['query']
>;
export type VerifyChainResult = Omit<
  operations['verify_integrity_chain']['responses'][200]['content']['application/json'],
  'chains'
> & {
  chains: IntegrityChains;
};

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
