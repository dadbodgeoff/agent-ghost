import { GhostAPIError } from '@ghost/sdk';
import { invalidateGhostClient } from '$lib/ghost-client';
import { getRuntime } from '$lib/platform/runtime';

export type AuthBoundaryMessageType =
  | 'ghost-auth-session'
  | 'ghost-auth-changed'
  | 'ghost-auth-cleared'
  | 'ghost-replay-pending-actions';

export interface AuthBoundaryState {
  client_id: string;
  session_epoch: number;
  token: string | null;
}

const PENDING_ACTIONS_DB = 'ghost-pending-actions';
const PENDING_ACTIONS_DB_VERSION = 2;
const PENDING_ACTIONS_STORE = 'pending_actions';
const AUTH_STATE_STORE = 'auth_state';
const AUTH_STATE_KEY = 'active';

export function isAuthResetError(error: unknown): boolean {
  return error instanceof GhostAPIError && (error.status === 401 || error.status === 403);
}

export function invalidateAuthClientState(): void {
  invalidateGhostClient();
}

export async function rotateAuthBoundarySession(): Promise<void> {
  const runtime = await getRuntime();
  await runtime.advanceReplaySessionEpoch();
}

export async function notifyAuthBoundary(type: AuthBoundaryMessageType): Promise<void> {
  if (typeof navigator === 'undefined' || !('serviceWorker' in navigator)) return;

  let auth: AuthBoundaryState | undefined;
  if (type !== 'ghost-auth-cleared' && type !== 'ghost-replay-pending-actions') {
    const runtime = await getRuntime();
    auth = {
      client_id: await runtime.getReplayClientId(),
      session_epoch: await runtime.getReplaySessionEpoch(),
      token: await runtime.getToken(),
    };
  }

  await applyDurableReplayBoundary(type, auth);

  const payload = auth ? { type, auth } : { type };
  navigator.serviceWorker.controller?.postMessage(payload);

  const registrations = await navigator.serviceWorker.getRegistrations().catch(() => []);
  for (const registration of registrations) {
    registration.active?.postMessage(payload);
    registration.waiting?.postMessage(payload);
    registration.installing?.postMessage(payload);
  }
}

async function applyDurableReplayBoundary(
  type: AuthBoundaryMessageType,
  auth?: AuthBoundaryState,
): Promise<void> {
  const db = await openReplayStateDb().catch(() => null);
  if (!db) return;

  await new Promise<void>((resolve) => {
    const storeNames =
      type === 'ghost-auth-session'
        ? [AUTH_STATE_STORE]
        : [PENDING_ACTIONS_STORE, AUTH_STATE_STORE];
    const tx = db.transaction(storeNames, 'readwrite');

    if (type === 'ghost-auth-session' && auth) {
      tx.objectStore(AUTH_STATE_STORE).put({ key: AUTH_STATE_KEY, ...auth });
    } else if (type === 'ghost-auth-changed') {
      tx.objectStore(PENDING_ACTIONS_STORE).clear();
      if (auth) {
        tx.objectStore(AUTH_STATE_STORE).put({ key: AUTH_STATE_KEY, ...auth });
      } else {
        tx.objectStore(AUTH_STATE_STORE).delete(AUTH_STATE_KEY);
      }
    } else if (type === 'ghost-auth-cleared') {
      tx.objectStore(PENDING_ACTIONS_STORE).clear();
      tx.objectStore(AUTH_STATE_STORE).delete(AUTH_STATE_KEY);
    }

    tx.oncomplete = () => resolve();
    tx.onabort = () => resolve();
    tx.onerror = () => resolve();
  });

  db.close();
}

function openReplayStateDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('IndexedDB unavailable'));
      return;
    }
    const request = indexedDB.open(PENDING_ACTIONS_DB, PENDING_ACTIONS_DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(PENDING_ACTIONS_STORE)) {
        db.createObjectStore(PENDING_ACTIONS_STORE, { keyPath: 'id', autoIncrement: true });
      }
      if (!db.objectStoreNames.contains(AUTH_STATE_STORE)) {
        db.createObjectStore(AUTH_STATE_STORE, { keyPath: 'key' });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
