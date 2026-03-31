/**
 * WebSocket singleton store — Svelte 5 runes.
 *
 * The SDK owns websocket transport concerns: auth, replay cursor handling,
 * reconnect backoff, and envelope normalization. This store keeps only
 * dashboard-specific orchestration: leader election, BroadcastChannel fan-out,
 * and event handler registration for view state.
 */

import { GhostWebSocket, type WsEventType } from '@ghost/sdk';
import { getRuntime, isTauriEnvironment } from '$lib/platform/runtime';

export type ConnectionState =
  | 'connecting'
  | 'connected'
  | 'disconnected'
  | 'reconnecting'
  | 'follower';

export interface WsMessage {
  type: string;
  [key: string]: unknown;
}

interface WsEnvelope {
  seq?: number;
  timestamp?: string;
  event?: WsMessage;
  type?: string;
}

type EventHandler = (msg: WsMessage) => void;
type ResyncHandler = () => void;

class WebSocketStore {
  state = $state<ConnectionState>('disconnected');
  lastMessage = $state<WsMessage | null>(null);
  lastError = $state('');
  reconnectAttempt = $state(0);
  resyncVersion = $state(0);

  private socket: GhostWebSocket | null = null;
  private handlers = new Map<WsEventType, Set<EventHandler>>();
  private resyncHandlers = new Set<ResyncHandler>();
  private isLeader = true;
  private bc: BroadcastChannel | null = null;
  private lastSeq = 0;
  private leaderElectionStarted = false;
  private leaderReady: Promise<void> | null = null;
  private leaderReadyResolve: (() => void) | null = null;

  on(type: WsEventType, handler: EventHandler): () => void {
    if (!this.handlers.has(type)) {
      this.handlers.set(type, new Set());
    }
    this.handlers.get(type)!.add(handler);
    return () => {
      this.handlers.get(type)?.delete(handler);
    };
  }

  onResync(handler: ResyncHandler): () => void {
    this.resyncHandlers.add(handler);
    return () => {
      this.resyncHandlers.delete(handler);
    };
  }

  async connect() {
    if (this.socket || this.state === 'connecting' || this.state === 'connected') {
      return;
    }

    await this.initLeaderElection();
    if (!this.isLeader) {
      this.state = 'follower';
      return;
    }

    const runtime = await getRuntime();
    const baseUrl = await runtime.getBaseUrl();
    const token = await runtime.getToken();

    this.lastError = '';
    this.reconnectAttempt = 0;
    this.socket = new GhostWebSocket(
      {
        baseUrl,
        token: token ?? undefined,
      },
      {
        initialLastSeq: this.lastSeq,
        onEnvelope: (envelope) => {
          this.routeEnvelope(envelope as WsEnvelope);
          if (this.isLeader && this.bc) {
            this.bc.postMessage(envelope);
          }
        },
        onStateChange: (state) => {
          if (!this.isLeader) {
            return;
          }
          this.state = state;
          if (state === 'connected') {
            this.reconnectAttempt = 0;
            this.lastError = '';
          }
        },
        onReconnectScheduled: (attempt) => {
          if (!this.isLeader) {
            return;
          }
          this.reconnectAttempt = attempt;
          this.state = 'reconnecting';
        },
        onReconnectFailed: () => {
          if (!this.isLeader) {
            return;
          }
          this.state = 'disconnected';
          this.lastError = 'Connection lost - click to reconnect';
        },
        onError: (message) => {
          this.lastError = message;
        },
      },
    ).connect();
  }

  async reconnect() {
    this.socket?.disconnect();
    this.socket = null;
    if (this.isLeader) {
      this.state = 'disconnected';
      this.lastError = '';
      this.reconnectAttempt = 0;
    }
    await this.connect();
  }

  disconnect() {
    this.socket?.disconnect();
    this.socket = null;
    this.state = 'disconnected';
    this.resetLeaderElection();
  }

  private async initLeaderElection() {
    if (isTauriEnvironment()) {
      this.isLeader = true;
      return;
    }
    if (this.leaderElectionStarted) {
      await this.leaderReady;
      return;
    }
    this.leaderElectionStarted = true;
    this.leaderReady = new Promise((resolve) => {
      this.leaderReadyResolve = resolve;
    });

    try {
      this.bc = new BroadcastChannel('ghost-ws-leader');
      this.bc.onmessage = (event: MessageEvent) => {
        if (this.isLeader) return;
        try {
          const envelope = event.data as WsEnvelope;
          this.routeEnvelope(envelope.event ? envelope : { event: envelope as WsMessage });
        } catch {
          // Ignore malformed broadcast payloads.
        }
      };
    } catch {
      this.isLeader = true;
      this.resolveLeaderReady();
      return;
    }

    if (typeof navigator !== 'undefined' && navigator.locks) {
      navigator.locks.request('ghost-ws-leader', { ifAvailable: true }, async (lock) => {
        if (lock) {
          this.isLeader = true;
          this.state = 'disconnected';
          this.resolveLeaderReady();
          return new Promise<void>(() => {});
        }

        this.becomeFollower();
        this.resolveLeaderReady();
        navigator.locks.request('ghost-ws-leader', async () => {
          this.becomeLeader();
          return new Promise<void>(() => {});
        });
      });
    } else {
      this.isLeader = true;
      this.resolveLeaderReady();
    }

    await this.leaderReady;
  }

  private becomeLeader() {
    if (this.isLeader && this.socket) return;
    this.isLeader = true;
    this.socket?.disconnect();
    this.socket = null;
    this.state = 'disconnected';
    this.lastError = '';
    this.reconnectAttempt = 0;
    void this.connect();
  }

  private becomeFollower() {
    this.isLeader = false;
    this.socket?.disconnect();
    this.socket = null;
    this.state = 'follower';
    this.lastError = '';
    this.reconnectAttempt = 0;
  }

  private resolveLeaderReady() {
    this.leaderReadyResolve?.();
    this.leaderReadyResolve = null;
  }

  private resetLeaderElection() {
    this.bc?.close();
    this.bc = null;
    this.isLeader = true;
    this.leaderElectionStarted = false;
    this.leaderReady = null;
    this.leaderReadyResolve = null;
  }

  private notifyResync() {
    this.resyncVersion += 1;
    for (const handler of this.resyncHandlers) {
      try {
        handler();
      } catch (error) {
        console.error('[ws] resync handler error:', error);
      }
    }
  }

  private routeEnvelope(envelope: WsEnvelope) {
    if (typeof envelope.seq === 'number' && envelope.seq > this.lastSeq) {
      this.lastSeq = envelope.seq;
    }

    const msg = envelope.event;
    if (!msg) return;

    this.lastMessage = msg;

    if (msg.type === 'Ping') {
      return;
    }

    if (msg.type === 'Resync') {
      console.warn(
        `[ws] Resync: missed ${(msg as { missed_events?: number }).missed_events ?? '?'} events — refreshing all stores`,
      );
      this.notifyResync();
    }

    const typeHandlers = this.handlers.get(msg.type as WsEventType);
    if (!typeHandlers) return;

    for (const handler of typeHandlers) {
      try {
        handler(msg);
      } catch (error) {
        console.error(`[ws] handler error for ${msg.type}:`, error);
      }
    }
  }
}

export const wsStore = new WebSocketStore();
