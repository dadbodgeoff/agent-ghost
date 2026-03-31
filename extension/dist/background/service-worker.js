/**
 * ITP event emitter — sends events to native messaging host or IndexedDB fallback.
 */
class ITPEmitter {
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
    emit(event) {
        if (this.useNative && this.nativePort) {
            this.nativePort.postMessage(event);
        }
        else {
            this.storeInIndexedDB(event);
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
/**
 * Background service worker — manages ITP emission and native messaging.
 */

const emitter = new ITPEmitter();
// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
    if (message.type === 'NEW_MESSAGE') {
        emitter.emit({
            eventType: 'InteractionMessage',
            platform: message.platform,
            role: message.role,
            contentHash: message.contentHash,
            timestamp: new Date().toISOString(),
            sessionId: message.sessionId,
        });
        sendResponse({ ok: true });
    }
    if (message.type === 'SESSION_START') {
        emitter.emit({
            eventType: 'SessionStart',
            platform: message.platform,
            timestamp: new Date().toISOString(),
            sessionId: message.sessionId,
        });
        sendResponse({ ok: true });
    }
    if (message.type === 'GET_SCORE') {
        sendResponse({ score: emitter.getLatestScore() });
    }
    return true; // Keep channel open for async response
});
// Periodic score refresh
setInterval(() => {
    emitter.refreshScore();
}, 30_000);
console.log('[GHOST] Background service worker initialized');
//# sourceMappingURL=service-worker.js.map

