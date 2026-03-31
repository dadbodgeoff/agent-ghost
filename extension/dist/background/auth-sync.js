/**
 * JWT auth sync between extension and GHOST dashboard (T-4.9.1).
 *
 * Reads JWT token from chrome.storage.local and validates it against
 * the gateway /api/health endpoint. Syncs auth state with dashboard.
 */
const GATEWAY_URL_KEY = 'ghost-gateway-url';
const JWT_TOKEN_KEY = 'ghost-jwt-token';
const currentState = {
    authenticated: false,
    gatewayUrl: 'http://localhost:39780',
    token: null,
    lastValidated: 0,
};
function normalizeGatewayUrl(value) {
    if (typeof value !== 'string') {
        return 'http://localhost:39780';
    }
    const trimmed = value.trim();
    return trimmed ? trimmed.replace(/\/+$/, '') : 'http://localhost:39780';
}
/**
 * Initialize auth sync — loads stored credentials and validates.
 */
export async function initAuthSync() {
    const stored = await chrome.storage.local.get([GATEWAY_URL_KEY, JWT_TOKEN_KEY]);
    currentState.gatewayUrl = normalizeGatewayUrl(stored[GATEWAY_URL_KEY]);
    currentState.token = typeof stored[JWT_TOKEN_KEY] === 'string' ? stored[JWT_TOKEN_KEY] : null;
    if (currentState.token) {
        await validateToken();
    }
    return currentState;
}
/**
 * Store JWT token from dashboard login.
 */
export async function storeToken(token, gatewayUrl) {
    currentState.token = token;
    if (gatewayUrl) {
        currentState.gatewayUrl = normalizeGatewayUrl(gatewayUrl);
    }
    await chrome.storage.local.set({
        [JWT_TOKEN_KEY]: token,
        [GATEWAY_URL_KEY]: currentState.gatewayUrl,
    });
    await validateToken();
}
/**
 * Clear stored token.
 */
export async function clearToken() {
    currentState.token = null;
    currentState.authenticated = false;
    currentState.lastValidated = Date.now();
    await chrome.storage.local.remove([JWT_TOKEN_KEY]);
}
/**
 * Validate the current token against the gateway.
 */
async function validateToken() {
    currentState.lastValidated = Date.now();
    if (!currentState.token) {
        currentState.authenticated = false;
        return false;
    }
    try {
        const resp = await fetch(`${currentState.gatewayUrl}/api/health`, {
            headers: {
                Authorization: `Bearer ${currentState.token}`,
            },
            signal: AbortSignal.timeout(5000),
        });
        currentState.authenticated = resp.ok;
        return resp.ok;
    }
    catch {
        currentState.authenticated = false;
        return false;
    }
}
/**
 * Get current auth state.
 */
export function getAuthState() {
    return { ...currentState };
}
//# sourceMappingURL=auth-sync.js.map
