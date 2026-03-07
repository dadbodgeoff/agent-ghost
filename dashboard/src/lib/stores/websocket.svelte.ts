/**
 * WebSocket singleton store — Svelte 5 runes.
 *
 * The SDK owns websocket transport concerns: auth, replay cursor handling,
 * reconnect backoff, and envelope normalization. This store keeps only
 * dashboard-specific orchestration: leader election, BroadcastChannel fan-out,
 * and event handler registration for view state.
 */

import { GhostWebSocket } from '@ghost/sdk';
import { getRuntime, isTauriEnvironment } from '$lib/platform/runtime';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

export type WsEventType =
  | 'ScoreUpdate'
  | 'InterventionChange'
  | 'KillSwitchActivation'
  | 'ProposalDecision'
  | 'AgentStateChange'
  | 'AgentConfigChange'
  | 'TraceUpdate'
  | 'BackupComplete'
  | 'WebhookFired'
  | 'SkillChange'
  | 'A2ATaskUpdate'
  | 'SessionEvent'
  | 'ChatMessage'
  | 'Resync'
  | 'Ping';

export interface WsMessage {
  type: WsEventType;
  [key: string]: unknown;
}

interface WsEnvelope {
  seq?: number;
  timestamp?: string;
  event?: WsMessage;
  type?: string;
}

type EventHandler = (msg: WsMessage) => void;

class WebSocketStore {
  state = $state<ConnectionState>('disconnected');
  lastMessage = $state<WsMessage | null>(null);
  lastError = $state('');
  reconnectAttempt = $state(0);

  private socket: GhostWebSocket | null = null;
  private handlers = new Map<WsEventType, Set<EventHandler>>();
  private isLeader = true;
  private bc: BroadcastChannel | null = null;
  private lastSeq = 0;

  on(type: WsEventType, handler: EventHandler): () => void {
    if (!this.handlers.has(type)) {
      this.handlers.set(type, new Set());
    }
    this.handlers.get(type)!.add(handler);
    return () => {
      this.handlers.get(type)?.delete(handler);
    };
  }

  async connect() {
    if (this.socket || this.state === 'connecting' || this.state === 'connected') {
      return;
    }

    this.initLeaderElection();
    if (!this.isLeader) return;

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
          this.state = state;
          if (state === 'connected') {
            this.reconnectAttempt = 0;
            this.lastError = '';
          }
          if (state === 'disconnected' && !this.isLeader) {
            this.state = 'connected';
          }
        },
        onReconnectScheduled: (attempt) => {
          this.reconnectAttempt = attempt;
          this.state = 'reconnecting';
        },
        onError: (message) => {
          this.lastError = message;
        },
      },
    ).connect();
  }

  disconnect() {
    this.socket?.disconnect();
    this.socket = null;
    this.state = 'disconnected';
    this.bc?.close();
    this.bc = null;
  }

  private initLeaderElection() {
    if (isTauriEnvironment()) return;
    if (this.bc) return;

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
      return;
    }

    if (typeof navigator !== 'undefined' && navigator.locks) {
      navigator.locks.request('ghost-ws-leader', { ifAvailable: true }, async (lock) => {
        if (lock) {
          this.becomeLeader();
          return new Promise<void>(() => {});
        }

        this.becomeFollower();
        navigator.locks.request('ghost-ws-leader', async () => {
          this.becomeLeader();
          return new Promise<void>(() => {});
        });
      });
    }
  }

  private becomeLeader() {
    if (this.isLeader) return;
    this.isLeader = true;
    void this.connect();
  }

  private becomeFollower() {
    if (!this.isLeader) return;
    this.isLeader = false;
    this.socket?.disconnect();
    this.socket = null;
    this.state = 'connected';
    this.lastError = '';
    this.reconnectAttempt = 0;
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
    }

    const typeHandlers = this.handlers.get(msg.type);
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
