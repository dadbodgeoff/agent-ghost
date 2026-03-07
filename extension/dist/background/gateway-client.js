/**
 * REST client for ghost-gateway (T-4.9.3).
 *
 * Provides typed API calls from the extension background to the gateway.
 * Forwards ITP observations and retrieves agent state.
 */
import { getAuthState } from './auth-sync';
/**
 * Make an authenticated request to the gateway.
 */
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
/**
 * Get gateway health status.
 */
export async function getHealth() {
    return request('/api/health');
}
/**
 * Get list of agents.
 */
export async function getAgents() {
    const data = await request('/api/agents');
    return data.agents || [];
}
/**
 * Get convergence scores.
 */
export async function getScores() {
    return request('/api/convergence/scores');
}
/**
 * Forward an ITP observation to the gateway.
 */
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
//# sourceMappingURL=gateway-client.js.map