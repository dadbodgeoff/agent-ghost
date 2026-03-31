/**
 * ITP event emitter — sends events to native messaging host or IndexedDB fallback.
 */

import { queueEvent } from '../storage/sync';

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
  private latestScore = 0;
  private useNative: boolean = false;
  private readonly scoreListeners = new Set<(score: number) => void>();

  constructor() {
    this.tryConnectNative();
  }

  onScoreChange(listener: (score: number) => void): () => void {
    this.scoreListeners.add(listener);
    return () => this.scoreListeners.delete(listener);
  }

  private setLatestScore(score: number): void {
    this.latestScore = score;
    for (const listener of this.scoreListeners) {
      listener(score);
    }
  }

  private tryConnectNative(): void {
    try {
      this.nativePort = chrome.runtime.connectNative('dev.ghost.monitor');
      this.nativePort.onMessage.addListener((msg: { score?: number }) => {
        if (typeof msg.score === 'number') {
          this.setLatestScore(msg.score);
        }
      });
      this.nativePort.onDisconnect.addListener(() => {
        console.log('[GHOST] Native messaging disconnected, falling back to IndexedDB');
        this.nativePort = null;
        this.useNative = false;
      });
      this.useNative = true;
    } catch {
      console.log('[GHOST] Native messaging unavailable, using IndexedDB fallback');
      this.useNative = false;
    }
  }

  emit(event: ITPEvent): void {
    if (this.useNative && this.nativePort) {
      this.nativePort.postMessage(event);
    } else {
      void this.storeInIndexedDB(event);
    }
  }

  getLatestScore(): number {
    return this.latestScore;
  }

  refreshScore(): void {
    if (this.useNative && this.nativePort) {
      this.nativePort.postMessage({ type: 'GET_SCORE' });
    }
  }

  private async storeInIndexedDB(event: ITPEvent): Promise<void> {
    await queueEvent('observation', {
      ...event,
      storedAt: new Date().toISOString(),
    });
  }
}
