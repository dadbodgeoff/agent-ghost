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

type JsonObject = Record<string, unknown>;

/**
 * Make an authenticated request to the gateway.
 */
async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
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
    const detail = await resp.text().catch(() => '');
    throw new Error(
      detail
        ? `Gateway ${resp.status}: ${resp.statusText} - ${detail}`
        : `Gateway ${resp.status}: ${resp.statusText}`,
    );
  }

  if (resp.status === 204 || resp.headers.get('content-length') === '0') {
    return undefined as T;
  }

  const body = await resp.text();
  if (!body.trim()) {
    return undefined as T;
  }

  return JSON.parse(body) as T;
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
