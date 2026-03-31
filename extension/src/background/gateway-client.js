/**
 * REST client for ghost-gateway.
 */

import { getAuthState } from './auth-sync.js';

async function request(path, options = {}) {
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

export async function getHealth() {
  return request('/api/health');
}

export async function getAgents() {
  const data = await request('/api/agents');
  return data.agents || [];
}

export async function getScores() {
  return request('/api/convergence/scores');
}

export async function forwardObservation(observation) {
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
