/**
 * JWT auth sync between extension and GHOST dashboard.
 */

const GATEWAY_URL_KEY = 'ghost-gateway-url';
const JWT_TOKEN_KEY = 'ghost-jwt-token';

const currentState = {
  authenticated: false,
  gatewayUrl: 'http://localhost:39780',
  token: null,
  lastValidated: 0,
};

export async function initAuthSync() {
  const stored = await chrome.storage.local.get([GATEWAY_URL_KEY, JWT_TOKEN_KEY]);
  currentState.gatewayUrl = stored[GATEWAY_URL_KEY] || 'http://localhost:39780';
  currentState.token = stored[JWT_TOKEN_KEY] || null;

  if (currentState.token) {
    await validateToken();
  } else {
    currentState.authenticated = false;
    currentState.lastValidated = 0;
  }

  return { ...currentState };
}

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

export async function clearToken() {
  currentState.token = null;
  currentState.authenticated = false;
  currentState.lastValidated = 0;
  await chrome.storage.local.remove([JWT_TOKEN_KEY]);
}

async function validateToken() {
  if (!currentState.token) {
    currentState.authenticated = false;
    currentState.lastValidated = 0;
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
  } catch {
    currentState.authenticated = false;
    return false;
  }
}

export function getAuthState() {
  return { ...currentState };
}
