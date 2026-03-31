/**
 * ITP event emitter — sends events to native messaging host or IndexedDB fallback.
 */
export class ITPEmitter {
    nativePort = null;
    latestScore = 0;
    useNative = false;
    constructor() {
        this.tryConnectNative();
    }
    tryConnectNative() {
        try {
            this.nativePort = chrome.runtime.connectNative('dev.ghost.monitor');
            this.nativePort.onMessage.addListener((msg) => {
                if (typeof msg.score === 'number') {
                    this.latestScore = msg.score;
                }
            });
            this.nativePort.onDisconnect.addListener(() => {
                console.log('[GHOST] Native messaging disconnected, falling back to IndexedDB');
                this.nativePort = null;
                this.useNative = false;
            });
            this.useNative = true;
        }
        catch {
            console.log('[GHOST] Native messaging unavailable, using IndexedDB fallback');
            this.useNative = false;
        }
    }
    async emit(event) {
        if (this.useNative && this.nativePort) {
            this.nativePort.postMessage(event);
        }
        else {
            await this.storeInIndexedDB(event);
        }
    }
    getLatestScore() {
        return this.latestScore;
    }
    refreshScore() {
        if (this.useNative && this.nativePort) {
            this.nativePort.postMessage({ type: 'GET_SCORE' });
        }
    }
    async storeInIndexedDB(event) {
        const db = await this.openDB();
        const tx = db.transaction('events', 'readwrite');
        tx.objectStore('events').add({
            ...event,
            storedAt: new Date().toISOString(),
        });
    }
    openDB() {
        return new Promise((resolve, reject) => {
            const request = indexedDB.open('ghost-itp', 1);
            request.onupgradeneeded = () => {
                const db = request.result;
                if (!db.objectStoreNames.contains('events')) {
                    db.createObjectStore('events', { autoIncrement: true });
                }
                if (!db.objectStoreNames.contains('settings')) {
                    db.createObjectStore('settings', { keyPath: 'key' });
                }
            };
            request.onsuccess = () => resolve(request.result);
            request.onerror = () => reject(request.error);
        });
    }
}
//# sourceMappingURL=itp-emitter.js.map
