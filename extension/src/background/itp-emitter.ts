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
  private latestScore: number = 0;
  private useNative: boolean = false;

  constructor() {
    this.tryConnectNative();
  }

  private tryConnectNative(): void {
    try {
      this.nativePort = chrome.runtime.connectNative('dev.ghost.monitor');
      this.nativePort.onMessage.addListener((msg: { score?: number }) => {
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
    } catch {
      console.log('[GHOST] Native messaging unavailable, using IndexedDB fallback');
      this.useNative = false;
    }
  }

  emit(event: ITPEvent): void {
    if (this.useNative && this.nativePort) {
      this.nativePort.postMessage(event);
    } else {
      void queueEvent(event.eventType, event);
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

}
