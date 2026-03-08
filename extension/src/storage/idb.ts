/**
 * IndexedDB storage for session data (AC6).
 */

const DB_NAME = 'ghost-itp';
const DB_VERSION = 1;

type StoredEvent = Record<string, unknown>;

export function openDB(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains('events')) {
        const store = db.createObjectStore('events', { autoIncrement: true });
        store.createIndex('timestamp', 'timestamp');
        store.createIndex('sessionId', 'sessionId');
      }
      if (!db.objectStoreNames.contains('sessions')) {
        db.createObjectStore('sessions', { keyPath: 'id' });
      }
      if (!db.objectStoreNames.contains('settings')) {
        db.createObjectStore('settings', { keyPath: 'key' });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

export async function storeEvent(event: StoredEvent): Promise<void> {
  const db = await openDB();
  const tx = db.transaction('events', 'readwrite');
  tx.objectStore('events').add(event);
}

export async function getEvents(sessionId: string): Promise<StoredEvent[]> {
  const db = await openDB();
  const tx = db.transaction('events', 'readonly');
  const index = tx.objectStore('events').index('sessionId');
  const request = index.getAll(sessionId);
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result as StoredEvent[]);
    request.onerror = () => reject(request.error);
  });
}

export async function getSetting(key: string): Promise<unknown> {
  const db = await openDB();
  const tx = db.transaction('settings', 'readonly');
  const request = tx.objectStore('settings').get(key);
  return new Promise((resolve, reject) => {
    request.onsuccess = () => {
      const result = request.result as { value?: unknown } | undefined;
      resolve(result?.value);
    };
    request.onerror = () => reject(request.error);
  });
}

export async function setSetting(key: string, value: unknown): Promise<void> {
  const db = await openDB();
  const tx = db.transaction('settings', 'readwrite');
  tx.objectStore('settings').put({ key, value });
}
