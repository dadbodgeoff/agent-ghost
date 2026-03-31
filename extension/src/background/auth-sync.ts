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

let hydratePromise: Promise<AuthState> | null = null;
const TOKEN_REVALIDATE_MS = 60_000;

function applyStoredState(stored: Record<string, unknown>): void {
  const gatewayUrl = stored[GATEWAY_URL_KEY];
  const token = stored[JWT_TOKEN_KEY];

  currentState.gatewayUrl = typeof gatewayUrl === 'string' && gatewayUrl.trim()
    ? gatewayUrl
    : 'http://localhost:39780';
  currentState.token = typeof token === 'string' && token.trim() ? token : null;
}

async function hydrateFromStorage(forceValidation = false): Promise<AuthState> {
  if (!hydratePromise) {
    hydratePromise = chrome.storage.local
      .get([GATEWAY_URL_KEY, JWT_TOKEN_KEY])
      .then(async (stored) => {
        applyStoredState(stored);

        const needsValidation = Boolean(currentState.token) && (
          forceValidation
          || currentState.lastValidated === 0
          || Date.now() - currentState.lastValidated > TOKEN_REVALIDATE_MS
        );

        if (needsValidation) {
          await validateToken();
        } else if (!currentState.token) {
          currentState.authenticated = false;
        }

        return { ...currentState };
      })
      .finally(() => {
        hydratePromise = null;
      });
  }

  return hydratePromise;
}

/**
 * Initialize auth sync — loads stored credentials and validates.
 */
export async function initAuthSync(): Promise<AuthState> {
  return hydrateFromStorage(true);
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
    return false;
  }
}

/**
 * Get current auth state.
 */
export async function getAuthState(): Promise<AuthState> {
  return hydrateFromStorage();
}

chrome.storage.onChanged.addListener((changes, areaName) => {
  if (areaName !== 'local') return;

  if (changes[GATEWAY_URL_KEY]) {
    const next = changes[GATEWAY_URL_KEY].newValue;
    currentState.gatewayUrl = typeof next === 'string' && next.trim()
      ? next
      : 'http://localhost:39780';
    currentState.lastValidated = 0;
  }

  if (changes[JWT_TOKEN_KEY]) {
    const next = changes[JWT_TOKEN_KEY].newValue;
    currentState.token = typeof next === 'string' && next.trim() ? next : null;
    currentState.authenticated = false;
    currentState.lastValidated = 0;
    if (currentState.token) {
      void validateToken();
    }
  }
});
