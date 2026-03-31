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
let listenerRegistered = false;
/**
 * Initialize auth sync — loads stored credentials and validates.
 */
export async function initAuthSync() {
    const stored = await chrome.storage.local.get([GATEWAY_URL_KEY, JWT_TOKEN_KEY]);
    currentState.gatewayUrl = stored[GATEWAY_URL_KEY] || 'http://localhost:39780';
    currentState.token = stored[JWT_TOKEN_KEY] || null;
    registerStorageListener();
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
        currentState.gatewayUrl = gatewayUrl;
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
        currentState.lastValidated = Date.now();
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
function registerStorageListener() {
    if (listenerRegistered) {
        return;
    }
    chrome.storage.onChanged.addListener((changes, areaName) => {
        if (areaName !== 'local') {
            return;
        }
        if (changes[GATEWAY_URL_KEY]) {
            currentState.gatewayUrl =
                changes[GATEWAY_URL_KEY].newValue || 'http://localhost:39780';
        }
        if (changes[JWT_TOKEN_KEY]) {
            currentState.token = changes[JWT_TOKEN_KEY].newValue || null;
            currentState.authenticated = false;
            currentState.lastValidated = Date.now();
            if (currentState.token) {
                void validateToken();
            }
        }
    });
    listenerRegistered = true;
}
//# sourceMappingURL=auth-sync.js.map
