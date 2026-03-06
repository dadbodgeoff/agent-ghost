/**
 * WebSocket singleton store — Svelte 5 runes.
 *
 * Manages a single WebSocket connection to the gateway with:
 * - Exponential backoff with jitter (1s → 30s, ×2 + random 0–1s)
 * - Connection state exposed as reactive $state
 * - Message routing by event `type` field to domain stores
 * - Ping/pong keepalive handling
 * - Multi-tab leader election via BroadcastChannel
 *
 * Ref: T-1.7.1, §5.1
 */

import { BASE_URL } from '$lib/api';
const WS_URL = BASE_URL.replace(/^http/, 'ws') + '/api/ws';
const INITIAL_BACKOFF_MS = 1000;
const MAX_BACKOFF_MS = 30_000;
const BACKOFF_MULTIPLIER = 2;

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
  | 'Resync'
  | 'Ping';

export interface WsMessage {
  type: WsEventType;
  [key: string]: unknown;
}

type EventHandler = (msg: WsMessage) => void;

class WebSocketStore {
  state = $state<ConnectionState>('disconnected');
  lastMessage = $state<WsMessage | null>(null);
  lastError = $state<string>('');
  reconnectAttempt = $state(0);

  private ws: WebSocket | null = null;
  private backoffMs = INITIAL_BACKOFF_MS;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private handlers = new Map<WsEventType, Set<EventHandler>>();
  private isLeader = true;
  private bc: BroadcastChannel | null = null;
  private intentionalClose = false;

  /** Subscribe to a specific WS event type. Returns an unsubscribe function. */
  on(type: WsEventType, handler: EventHandler): () => void {
    if (!this.handlers.has(type)) {
      this.handlers.set(type, new Set());
    }
    this.handlers.get(type)!.add(handler);
    return () => {
      this.handlers.get(type)?.delete(handler);
    };
  }

  /** Connect to the gateway WebSocket. */
  connect() {
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    this.initLeaderElection();

    const token = sessionStorage.getItem('ghost-token');
    const url = token ? `${WS_URL}?token=${encodeURIComponent(token)}` : WS_URL;

    this.state = 'connecting';
    this.intentionalClose = false;

    try {
      this.ws = new WebSocket(url);
    } catch {
      this.state = 'disconnected';
      this.scheduleReconnect();
      return;
    }

    this.ws.onopen = () => {
      this.state = 'connected';
      this.backoffMs = INITIAL_BACKOFF_MS;
      this.reconnectAttempt = 0;
      this.lastError = '';
    };

    this.ws.onmessage = (event: MessageEvent) => {
      try {
        const msg: WsMessage = JSON.parse(event.data);
        this.lastMessage = msg;

        if (msg.type === 'Ping') {
          // Keepalive — no action needed beyond updating lastMessage.
          return;
        }

        // T-5.3.4 (T-X.28): Handle Resync — client lagged behind broadcast.
        // Trigger full REST re-fetch on all stores to guarantee consistency.
        if (msg.type === 'Resync') {
          console.warn(`[ws] Resync: missed ${(msg as { missed_events?: number }).missed_events ?? '?'} events — refreshing all stores`);
          // Route to registered Resync handlers (pages should re-fetch their data).
          const resyncHandlers = this.handlers.get('Resync');
          if (resyncHandlers) {
            for (const handler of resyncHandlers) {
              try { handler(msg); } catch (e) { console.error('[ws] Resync handler error:', e); }
            }
          }
          return;
        }

        // Route to registered handlers.
        const typeHandlers = this.handlers.get(msg.type);
        if (typeHandlers) {
          for (const handler of typeHandlers) {
            try {
              handler(msg);
            } catch (e) {
              console.error(`[ws] handler error for ${msg.type}:`, e);
            }
          }
        }

        // Broadcast to other tabs if we're the leader.
        if (this.isLeader && this.bc) {
          this.bc.postMessage(msg);
        }
      } catch {
        // Non-JSON message — ignore.
      }
    };

    this.ws.onclose = (event: CloseEvent) => {
      this.ws = null;
      if (!this.intentionalClose) {
        this.state = 'reconnecting';
        this.scheduleReconnect();
      } else {
        this.state = 'disconnected';
      }
    };

    this.ws.onerror = () => {
      this.lastError = 'WebSocket connection error';
    };
  }

  /** Disconnect intentionally (no reconnect). */
  disconnect() {
    this.intentionalClose = true;
    this.clearReconnectTimer();
    if (this.ws) {
      this.ws.close(1000, 'Client disconnect');
      this.ws = null;
    }
    this.state = 'disconnected';
    this.bc?.close();
    this.bc = null;
  }

  private scheduleReconnect() {
    this.clearReconnectTimer();
    const jitter = Math.random() * 1000;
    const delay = Math.min(this.backoffMs + jitter, MAX_BACKOFF_MS);
    this.reconnectAttempt++;

    this.reconnectTimer = setTimeout(() => {
      this.backoffMs = Math.min(this.backoffMs * BACKOFF_MULTIPLIER, MAX_BACKOFF_MS);
      this.connect();
    }, delay);
  }

  private clearReconnectTimer() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  /** Multi-tab leader election via BroadcastChannel. */
  private initLeaderElection() {
    // Skip in Tauri — single window, no leader election needed
    if (typeof window !== 'undefined' && (window as any).__TAURI__) return;
    if (this.bc) return;
    try {
      this.bc = new BroadcastChannel('ghost-ws-leader');
      this.bc.onmessage = (event: MessageEvent) => {
        // If we receive a message from another tab, we're a follower.
        // Process the forwarded WS message.
        if (!this.isLeader) {
          const msg = event.data as WsMessage;
          this.lastMessage = msg;
          const typeHandlers = this.handlers.get(msg.type);
          if (typeHandlers) {
            for (const handler of typeHandlers) {
              try { handler(msg); } catch { /* swallow */ }
            }
          }
        }
      };
    } catch {
      // BroadcastChannel not supported — single-tab mode.
    }
  }
}

/** Singleton WebSocket store instance. */
export const wsStore = new WebSocketStore();
