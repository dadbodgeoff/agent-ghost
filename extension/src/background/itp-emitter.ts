/**
 * ITP event emitter — sends events to native messaging host or IndexedDB fallback.
 */

interface ITPEvent {
  eventType: string;
  platform: string;
  timestamp: string;
  sessionId?: string;
  role?: string;
  contentHash?: string;
}

export class ITPEmitter {
  private nativePort: chrome.runtime.Port | null = null;
  private latestScore: number = 0;
  private useNative: boolean = false;
  private reconnectTimer: number | null = null;
  private scoreListeners = new Set<(score: number) => void>();

  constructor() {
    this.tryConnectNative();
  }

  private tryConnectNative(): void {
    try {
      this.nativePort = chrome.runtime.connectNative('dev.ghost.monitor');
      this.nativePort.onMessage.addListener((msg: { score?: number }) => {
        if (typeof msg.score === 'number') {
          this.latestScore = msg.score;
          this.notifyScoreListeners();
        }
      });
      this.nativePort.onDisconnect.addListener(() => {
        console.log('[GHOST] Native messaging disconnected, falling back to IndexedDB');
        this.nativePort = null;
        this.useNative = false;
        this.scheduleReconnect();
      });
      this.useNative = true;
      if (this.reconnectTimer !== null) {
        clearTimeout(this.reconnectTimer);
        this.reconnectTimer = null;
      }
    } catch {
      console.log('[GHOST] Native messaging unavailable, using IndexedDB fallback');
      this.useNative = false;
      this.scheduleReconnect();
    }
  }

  emit(event: ITPEvent): void {
    try {
      if (this.useNative && this.nativePort) {
        this.nativePort.postMessage(event);
        return;
      }
    } catch {
      this.nativePort = null;
      this.useNative = false;
      this.scheduleReconnect();
    }

    void this.storeInIndexedDB(event);
  }

  getLatestScore(): number {
    return this.latestScore;
  }

  refreshScore(): void {
    if (this.useNative && this.nativePort) {
      this.nativePort.postMessage({ type: 'GET_SCORE' });
    }
  }

  onScoreUpdate(listener: (score: number) => void): () => void {
    this.scoreListeners.add(listener);
    return () => {
      this.scoreListeners.delete(listener);
    };
  }

  private notifyScoreListeners(): void {
    for (const listener of this.scoreListeners) {
      listener(this.latestScore);
    }
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer !== null) {
      return;
    }

    this.reconnectTimer = self.setTimeout(() => {
      this.reconnectTimer = null;
      this.tryConnectNative();
    }, 15_000);
  }

  private async storeInIndexedDB(event: ITPEvent): Promise<void> {
    const db = await this.openDB();
    const tx = db.transaction('events', 'readwrite');
    tx.objectStore('events').add({
      ...event,
      storedAt: new Date().toISOString(),
    });
  }

  private openDB(): Promise<IDBDatabase> {
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
