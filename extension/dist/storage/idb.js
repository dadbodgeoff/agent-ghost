/**
 * IndexedDB storage for session data (AC6).
 */
const DB_NAME = 'ghost-itp';
const DB_VERSION = 1;
export function openDB() {
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
export async function storeEvent(event) {
    const db = await openDB();
    const tx = db.transaction('events', 'readwrite');
    tx.objectStore('events').add(event);
}
export async function getEvents(sessionId) {
    const db = await openDB();
    const tx = db.transaction('events', 'readonly');
    const index = tx.objectStore('events').index('sessionId');
    const request = index.getAll(sessionId);
    return new Promise((resolve, reject) => {
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}
export async function getSetting(key) {
    const db = await openDB();
    const tx = db.transaction('settings', 'readonly');
    const request = tx.objectStore('settings').get(key);
    return new Promise((resolve, reject) => {
        request.onsuccess = () => resolve(request.result?.value);
        request.onerror = () => reject(request.error);
    });
}
export async function setSetting(key, value) {
    const db = await openDB();
    const tx = db.transaction('settings', 'readwrite');
    tx.objectStore('settings').put({ key, value });
}
//# sourceMappingURL=idb.js.map