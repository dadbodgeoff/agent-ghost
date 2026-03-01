/**
 * Auth utilities — token stored in sessionStorage (cleared on tab close).
 *
 * For JWT mode, the access token is stored here after POST /api/auth/login.
 * For legacy mode, the GHOST_TOKEN is stored directly.
 *
 * Note: .svelte.ts stores cannot be imported from regular .ts files,
 * so this module stays as plain .ts for use by api.ts and layout.
 */

const TOKEN_KEY = 'ghost-token';

export function isAuthenticated(): boolean {
  return sessionStorage.getItem(TOKEN_KEY) !== null;
}

export function getToken(): string | null {
  return sessionStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  sessionStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  sessionStorage.removeItem(TOKEN_KEY);
}
