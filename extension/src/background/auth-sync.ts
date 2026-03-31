/**
 * JWT auth sync between extension and GHOST dashboard (T-4.9.1).
 *
 * Reads JWT token from chrome.storage.local and validates it against
 * the authenticated session endpoint. Syncs auth state across extension contexts.
 */

import { GATEWAY_URL_KEY, JWT_TOKEN_KEY } from './auth-keys';

const DEFAULT_GATEWAY_URL = 'http://localhost:39780';

export interface AuthState {
  authenticated: boolean;
  gatewayUrl: string;
  token: string | null;
  lastValidated: number;
}

type AuthStorageSnapshot = Record<string, string | null | undefined>;

const currentState: AuthState = {
  authenticated: false,
  gatewayUrl: DEFAULT_GATEWAY_URL,
  token: null,
  lastValidated: 0,
};
let storageListenerRegistered = false;

function applyStoredState(stored: AuthStorageSnapshot): void {
  currentState.gatewayUrl = stored[GATEWAY_URL_KEY] || DEFAULT_GATEWAY_URL;
  currentState.token = stored[JWT_TOKEN_KEY] || null;
}

function ensureStorageListener(): void {
  if (storageListenerRegistered) return;

  chrome.storage.onChanged.addListener((changes, areaName) => {
    if (areaName !== 'local') return;
    if (!changes[GATEWAY_URL_KEY] && !changes[JWT_TOKEN_KEY]) return;

    applyStoredState({
      [GATEWAY_URL_KEY]: changes[GATEWAY_URL_KEY]?.newValue,
      [JWT_TOKEN_KEY]: changes[JWT_TOKEN_KEY]?.newValue,
    });

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
  ensureStorageListener();

  const stored = await chrome.storage.local.get([GATEWAY_URL_KEY, JWT_TOKEN_KEY]);
  applyStoredState(stored);

  if (currentState.token) {
    await validateToken();
  } else {
    currentState.authenticated = false;
    currentState.lastValidated = 0;
  }

  return getAuthState();
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
    currentState.lastValidated = 0;
    return false;
  }

  try {
    const resp = await fetch(`${currentState.gatewayUrl}/api/auth/session`, {
      headers: {
        Authorization: `Bearer ${currentState.token}`,
        Accept: 'application/json',
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
