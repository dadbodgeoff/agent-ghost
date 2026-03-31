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

function normalizeStoredAuth(stored: Record<string, unknown>): AuthState {
  return {
    authenticated: false,
    gatewayUrl:
      typeof stored[GATEWAY_URL_KEY] === 'string' && stored[GATEWAY_URL_KEY]
        ? (stored[GATEWAY_URL_KEY] as string)
        : 'http://localhost:39780',
    token: typeof stored[JWT_TOKEN_KEY] === 'string' ? (stored[JWT_TOKEN_KEY] as string) : null,
    lastValidated: 0,
  };
}

async function loadStoredState(): Promise<AuthState> {
  const stored = await chrome.storage.local.get([GATEWAY_URL_KEY, JWT_TOKEN_KEY]);
  return normalizeStoredAuth(stored);
}

/**
 * Initialize auth sync — loads stored credentials and validates.
 */
export async function initAuthSync(): Promise<AuthState> {
  const storedState = await loadStoredState();
  currentState.gatewayUrl = storedState.gatewayUrl;
  currentState.token = storedState.token;

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

async function validateState(state: AuthState): Promise<AuthState> {
  if (!state.token) {
    return {
      ...state,
      authenticated: false,
      lastValidated: 0,
    };
  }

  try {
    const resp = await fetch(`${state.gatewayUrl}/api/health`, {
      headers: {
        Authorization: `Bearer ${state.token}`,
      },
      signal: AbortSignal.timeout(5000),
    });

    return {
      ...state,
      authenticated: resp.ok,
      lastValidated: Date.now(),
    };
  } catch {
    return {
      ...state,
      authenticated: false,
      lastValidated: state.lastValidated,
    };
  }
}

/**
 * Get current auth state.
 */
export function getAuthState(): AuthState {
  return { ...currentState };
}

/**
 * Resolve auth state for non-background extension contexts like the popup.
 */
export async function getAuthStateSnapshot(): Promise<AuthState> {
  const storedState = await loadStoredState();
  const validatedState = await validateState(storedState);

  currentState.gatewayUrl = validatedState.gatewayUrl;
  currentState.token = validatedState.token;
  currentState.authenticated = validatedState.authenticated;
  currentState.lastValidated = validatedState.lastValidated;

  return { ...validatedState };
}
