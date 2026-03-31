/**
 * JWT auth sync between extension and GHOST dashboard (T-4.9.1).
 *
 * Reads JWT token from chrome.storage.local and validates it against
 * the gateway /api/health endpoint. Syncs auth state with dashboard.
 */

const GATEWAY_URL_KEY = 'ghost-gateway-url';
const JWT_TOKEN_KEY = 'ghost-jwt-token';
const DEFAULT_GATEWAY_URL = 'http://localhost:39780';

export interface AuthState {
  authenticated: boolean;
  gatewayUrl: string;
  token: string | null;
  lastValidated: number;
}

const currentState: AuthState = {
  authenticated: false,
  gatewayUrl: DEFAULT_GATEWAY_URL,
  token: null,
  lastValidated: 0,
};
let storageSyncInitialized = false;

function normalizeGatewayUrl(url: string | undefined): string {
  return (url || DEFAULT_GATEWAY_URL).replace(/\/+$/, '');
}

function initStorageSync(): void {
  if (storageSyncInitialized) {
    return;
  }
  storageSyncInitialized = true;

  chrome.storage.onChanged.addListener((changes, areaName) => {
    if (areaName !== 'local') {
      return;
    }

    if (changes[GATEWAY_URL_KEY]) {
      currentState.gatewayUrl = normalizeGatewayUrl(changes[GATEWAY_URL_KEY].newValue as string | undefined);
    }
    if (changes[JWT_TOKEN_KEY]) {
      currentState.token = (changes[JWT_TOKEN_KEY].newValue as string | undefined) || null;
    }

    if (changes[GATEWAY_URL_KEY] || changes[JWT_TOKEN_KEY]) {
      void validateToken();
    }
  });
}

/**
 * Initialize auth sync — loads stored credentials and validates.
 */
export async function initAuthSync(): Promise<AuthState> {
  initStorageSync();
  const stored = await chrome.storage.local.get([GATEWAY_URL_KEY, JWT_TOKEN_KEY]);
  currentState.gatewayUrl = normalizeGatewayUrl(stored[GATEWAY_URL_KEY]);
  currentState.token = stored[JWT_TOKEN_KEY] || null;

  if (currentState.token) {
    await validateToken();
  }

  return currentState;
}

/**
 * Store JWT token from dashboard login.
 */
export async function storeToken(token: string, gatewayUrl?: string): Promise<void> {
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
export async function clearToken(): Promise<void> {
  currentState.token = null;
  currentState.authenticated = false;
  currentState.lastValidated = 0;
  await chrome.storage.local.remove([JWT_TOKEN_KEY]);
}

/**
 * Validate the current token against the gateway.
 */
async function validateToken(): Promise<boolean> {
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
  } catch {
    currentState.authenticated = false;
    currentState.lastValidated = Date.now();
    return false;
  }
}

/**
 * Get current auth state.
 */
export function getAuthState(): AuthState {
  return { ...currentState };
}
