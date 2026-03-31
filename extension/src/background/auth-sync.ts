/**
 * JWT auth sync between extension and GHOST dashboard (T-4.9.1).
 *
 * Reads JWT token from chrome.storage.local and validates it against
 * the gateway /api/health endpoint. Syncs auth state with dashboard.
 */

const GATEWAY_URL_KEY = 'ghost-gateway-url';
const JWT_TOKEN_KEY = 'ghost-jwt-token';

export interface AuthState {
  authenticated: boolean;
  gatewayUrl: string;
  token: string | null;
  lastValidated: number;
}

const currentState: AuthState = {
  authenticated: false,
  gatewayUrl: 'http://localhost:39780',
  token: null,
  lastValidated: 0,
};

let initializationPromise: Promise<AuthState> | null = null;
let storageListenerRegistered = false;

function registerStorageListener(): void {
  if (storageListenerRegistered || !chrome.storage?.onChanged) {
    return;
  }

  chrome.storage.onChanged.addListener((changes, areaName) => {
    if (areaName !== 'local') {
      return;
    }

    if (JWT_TOKEN_KEY in changes) {
      currentState.token = (changes[JWT_TOKEN_KEY]?.newValue as string | undefined) || null;
    }

    if (GATEWAY_URL_KEY in changes) {
      currentState.gatewayUrl =
        (changes[GATEWAY_URL_KEY]?.newValue as string | undefined) || 'http://localhost:39780';
    }

    if (!currentState.token) {
      currentState.authenticated = false;
      currentState.lastValidated = 0;
      return;
    }

    void validateToken();
  });

  storageListenerRegistered = true;
}

/**
 * Initialize auth sync — loads stored credentials and validates.
 */
export async function initAuthSync(): Promise<AuthState> {
  if (initializationPromise) {
    return initializationPromise;
  }

  registerStorageListener();

  initializationPromise = (async () => {
    const stored = await chrome.storage.local.get([GATEWAY_URL_KEY, JWT_TOKEN_KEY]);
    currentState.gatewayUrl = stored[GATEWAY_URL_KEY] || 'http://localhost:39780';
    currentState.token = stored[JWT_TOKEN_KEY] || null;

    if (currentState.token) {
      await validateToken();
    }

    return currentState;
  })();

  try {
    return await initializationPromise;
  } finally {
    initializationPromise = null;
  }
}

export async function ensureAuthStateLoaded(): Promise<AuthState> {
  registerStorageListener();

  if (!currentState.lastValidated && !initializationPromise) {
    return initAuthSync();
  }

  if (initializationPromise) {
    return initializationPromise;
  }

  return currentState;
}

/**
 * Store JWT token from dashboard login.
 */
export async function storeToken(token: string, gatewayUrl?: string): Promise<void> {
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
export async function clearToken(): Promise<void> {
  currentState.token = null;
  currentState.authenticated = false;
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
    return false;
  }
}

/**
 * Get current auth state.
 */
export function getAuthState(): AuthState {
  return { ...currentState };
}
