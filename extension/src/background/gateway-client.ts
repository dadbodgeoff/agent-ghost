/**
 * REST client for ghost-gateway (T-4.9.3).
 *
 * Provides typed API calls from the extension background to the gateway.
 * Forwards ITP observations and retrieves agent state.
 */

import { getAuthState, initAuthSync } from './auth-sync';

export interface AgentSummary {
  id: string;
  name: string;
  state: string;
}

export interface GatewayHealth {
  status: string;
  version?: string;
}

type JsonObject = Record<string, unknown>;

/**
 * Make an authenticated request to the gateway.
 */
async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const existingAuth = getAuthState();
  const auth = existingAuth.token ? existingAuth : await initAuthSync();
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

  return (await resp.json()) as T;
}

/**
 * Get gateway health status.
 */
export async function getHealth(): Promise<GatewayHealth> {
  return request<GatewayHealth>('/api/health');
}

/**
 * Get list of agents.
 */
export async function getAgents(): Promise<AgentSummary[]> {
  const data = await request<{ agents?: AgentSummary[] }>('/api/agents');
  return data.agents || [];
}

/**
 * Get convergence scores.
 */
export async function getScores(): Promise<JsonObject> {
  return request<JsonObject>('/api/convergence/scores');
}

/**
 * Forward an ITP observation to the gateway.
 */
export async function forwardObservation(observation: {
  platform: string;
  signal_type: string;
  value: number;
  metadata?: JsonObject;
}): Promise<void> {
  await request<void>('/api/memory', {
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
