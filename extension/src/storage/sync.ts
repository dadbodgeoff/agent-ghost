/**
 * IndexedDB <-> Gateway sync (T-4.9.4).
 *
 * Syncs pending IDB events to the gateway on reconnect.
 * Events queued while offline are replayed in order.
 */

import { getAuthState } from '../background/auth-sync';

const DB_NAME = 'ghost-convergence';
const PENDING_STORE = 'pending_events';

interface PendingEvent {
  id: number;
  timestamp: number;
  type: string;
  payload: unknown;
  synced: boolean;
}

interface NavigatorWithConnection extends Navigator {
  connection?: {
    addEventListener(type: 'change', listener: () => void): void;
  };
}

function waitForTransaction(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

/**
 * Open the IndexedDB database.
 */
function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, 2);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(PENDING_STORE)) {
        const store = db.createObjectStore(PENDING_STORE, { keyPath: 'id', autoIncrement: true });
        store.createIndex('synced', 'synced');
        store.createIndex('timestamp', 'timestamp');
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

/**
 * Queue an event for sync.
 */
export async function queueEvent(type: string, payload: unknown): Promise<void> {
  const db = await openDB();
  const tx = db.transaction(PENDING_STORE, 'readwrite');
  tx.objectStore(PENDING_STORE).add({
    timestamp: Date.now(),
    type,
    payload,
    synced: false,
  });
  await waitForTransaction(tx);
}

/**
 * Sync all pending events to the gateway.
 */
export async function syncPendingEvents(): Promise<{ synced: number; failed: number }> {
  const auth = getAuthState();
  if (!auth.authenticated || !auth.token) {
    return { synced: 0, failed: 0 };
  }

  const db = await openDB();
  const tx = db.transaction(PENDING_STORE, 'readonly');
  const store = tx.objectStore(PENDING_STORE);
  const index = store.index('synced');
  const request = index.getAll(IDBKeyRange.only(false));

  return new Promise((resolve) => {
    request.onsuccess = async () => {
      const events: PendingEvent[] = request.result || [];
      let synced = 0;
      let failed = 0;

      for (const event of events) {
        try {
          const response = await fetch(`${auth.gatewayUrl}/api/memory`, {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              Authorization: `Bearer ${auth.token}`,
            },
            body: JSON.stringify({
              type: event.type,
              content: JSON.stringify(event.payload),
              metadata: { source: 'extension-sync', original_timestamp: event.timestamp },
            }),
            signal: AbortSignal.timeout(5000),
          });

          if (!response.ok) {
            throw new Error(`Sync failed with ${response.status}`);
          }

          // Mark as synced.
          const updateTx = db.transaction(PENDING_STORE, 'readwrite');
          const updateStore = updateTx.objectStore(PENDING_STORE);
          updateStore.put({ ...event, synced: true });
          await waitForTransaction(updateTx);
          synced++;
        } catch {
          failed++;
          break; // Stop on first failure — retry later.
        }
      }

      resolve({ synced, failed });
    };
    request.onerror = () => resolve({ synced: 0, failed: 0 });
  });
}

/**
 * Clean up synced events older than 24 hours.
 */
export async function cleanupSyncedEvents(): Promise<void> {
  const db = await openDB();
  const cutoff = Date.now() - 24 * 60 * 60 * 1000;
  const tx = db.transaction(PENDING_STORE, 'readwrite');
  const store = tx.objectStore(PENDING_STORE);
  const index = store.index('timestamp');

  const request = index.openCursor(IDBKeyRange.upperBound(cutoff));
  request.onsuccess = () => {
    const cursor = request.result;
    if (cursor) {
      const event = cursor.value as PendingEvent;
      if (event.synced) {
        cursor.delete();
      }
      cursor.continue();
    }
  };
}

/**
 * Initialize auto-sync on reconnect.
 *
 * Listens for the browser "online" event and triggers a sync of
 * any queued events when connectivity is restored. Also watches
 * `navigator.connection` change events if available.
 */
export function initAutoSync(): void {
  // Sync pending events whenever the browser comes back online.
  self.addEventListener('online', async () => {
    const result = await syncPendingEvents();
    if (result.synced > 0) {
      await chrome.storage.local.set({ 'ghost-last-sync': Date.now() });
    }
  });

  // If the Network Information API is available, also listen for
  // connection-type changes (e.g. switching from cellular to wifi).
  const nav = navigator as NavigatorWithConnection;
  if (nav.connection) {
    nav.connection.addEventListener('change', async () => {
      if (navigator.onLine) {
        const result = await syncPendingEvents();
        if (result.synced > 0) {
          await chrome.storage.local.set({ 'ghost-last-sync': Date.now() });
        }
      }
    });
  }
}
