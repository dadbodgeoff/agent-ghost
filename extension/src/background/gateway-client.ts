/**
 * REST client for ghost-gateway (T-4.9.3).
 *
 * Provides typed API calls from the extension background to the gateway.
 * Forwards ITP observations and retrieves agent state.
 */

import { getAuthState } from './auth-sync';

export interface AgentSummary {
  id: string;
  name: string;
  state: string;
}

export interface GatewayHealth {
  status: string;
  version?: string;
}

/**
 * Make an authenticated request to the gateway.
 */
async function request(path: string, options: RequestInit = {}): Promise<any> {
  const auth = getAuthState();
  if (!auth.authenticated || !auth.token) {
    throw new Error('Not authenticated with gateway');
  }

  const resp = await fetch(`${auth.gatewayUrl}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${auth.token}`,
      ...(options.headers || {}),
    },
    signal: AbortSignal.timeout(10000),
  });

  if (!resp.ok) {
    throw new Error(`Gateway ${resp.status}: ${resp.statusText}`);
  }

  return resp.json();
}

/**
 * Get gateway health status.
 */
export async function getHealth(): Promise<GatewayHealth> {
  return request('/api/health');
}

/**
 * Get list of agents.
 */
export async function getAgents(): Promise<AgentSummary[]> {
  const data = await request('/api/agents');
  return data.agents || [];
}

/**
 * Get convergence scores.
 */
export async function getScores(): Promise<any> {
  return request('/api/convergence/scores');
}

/**
 * Forward an ITP observation to the gateway.
 */
export async function forwardObservation(observation: {
  platform: string;
  signal_type: string;
  value: number;
  metadata?: Record<string, any>;
}): Promise<void> {
  await request('/api/memory', {
    method: 'POST',
    body: JSON.stringify({
      type: 'observation',
      content: JSON.stringify(observation),
      metadata: {
        source: 'extension',
        platform: observation.platform,
        signal_type: observation.signal_type,
      },
    }),
  });
}
