/**
 * Auth utilities — token stored in sessionStorage (cleared on tab close).
 */

export function isAuthenticated(): boolean {
  return sessionStorage.getItem('ghost-token') !== null;
}

export function getToken(): string | null {
  return sessionStorage.getItem('ghost-token');
}

export function setToken(token: string): void {
  sessionStorage.setItem('ghost-token', token);
}

export function clearToken(): void {
  sessionStorage.removeItem('ghost-token');
}
