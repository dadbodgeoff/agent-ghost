import { GhostAPIError } from '@ghost/sdk';
import { invalidateGhostClient } from '$lib/ghost-client';

export type AuthBoundaryMessageType = 'ghost-auth-changed' | 'ghost-auth-cleared';

export function isAuthResetError(error: unknown): boolean {
  return error instanceof GhostAPIError && (error.status === 401 || error.status === 403);
}

export function invalidateAuthClientState(): void {
  invalidateGhostClient();
}

export async function notifyAuthBoundary(type: AuthBoundaryMessageType): Promise<void> {
  if (typeof navigator === 'undefined' || !('serviceWorker' in navigator)) return;

  const payload = { type };
  navigator.serviceWorker.controller?.postMessage(payload);

  const registrations = await navigator.serviceWorker.getRegistrations().catch(() => []);
  for (const registration of registrations) {
    registration.active?.postMessage(payload);
    registration.waiting?.postMessage(payload);
    registration.installing?.postMessage(payload);
  }
}
