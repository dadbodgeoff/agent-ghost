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

import { getRuntime } from '$lib/platform/runtime';
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
  | 'SessionEvent'
  | 'ChatMessage'
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
  private lastSeq = 0;

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
  async connect() {
    if (this.ws && (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING)) {
      return;
    }

    this.initLeaderElection();

    // Followers don't open their own WS — they receive events via BroadcastChannel.
    if (!this.isLeader) return;

    const runtime = await getRuntime();
    const baseUrl = await runtime.getBaseUrl();
    const token = await runtime.getToken();
    const wsUrl = baseUrl.replace(/^http/, 'ws') + '/api/ws';
    const url = token ? `${wsUrl}?token=${encodeURIComponent(token)}` : wsUrl;

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

      // Task 1.6: On reconnect, send last_seq to request missed event replay.
      if (this.lastSeq > 0 && this.ws) {
        this.ws.send(JSON.stringify({ last_seq: this.lastSeq }));
      }
    };

    this.ws.onmessage = (event: MessageEvent) => {
      try {
        // Task 1.6: Parse envelope and extract nested event.
        // Wire format: { seq, timestamp, event: { type, ...fields } }
        const envelope = JSON.parse(event.data) as {
          seq?: number;
          timestamp?: string;
          event?: WsMessage;
          // Backward compat: flattened format has `type` at top level.
          type?: string;
        };
        if (envelope.seq && envelope.seq > this.lastSeq) {
          this.lastSeq = envelope.seq;
        }
        // Extract inner event. Support both nested and legacy flat formats.
        const msg: WsMessage = envelope.event ?? (envelope as unknown as WsMessage);
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
      void this.connect();
    }, delay);
  }

  private clearReconnectTimer() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  /**
   * Multi-tab leader election via Web Locks API + BroadcastChannel.
   *
   * The tab that holds the 'ghost-ws-leader' lock is the leader and
   * owns the WebSocket connection. Follower tabs close their WS and
   * receive events via BroadcastChannel instead.
   *
   * If Web Locks is not available (older browsers), all tabs are leaders
   * (each opens its own WS — safe but wasteful).
   */
  private initLeaderElection() {
    // Skip in Tauri — single window, no leader election needed.
    if (typeof window !== 'undefined' && window.__TAURI__) return;
    if (this.bc) return;

    try {
      this.bc = new BroadcastChannel('ghost-ws-leader');
      this.bc.onmessage = (event: MessageEvent) => {
        if (this.isLeader) return; // Leaders ignore broadcast — they have their own WS.
        const msg = event.data as WsMessage;
        this.lastMessage = msg;

        // Update lastSeq from forwarded messages so follower→leader
        // promotion can reconnect from the right position.
        if ((msg as unknown as { seq?: number }).seq) {
          const seq = (msg as unknown as { seq: number }).seq;
          if (seq > this.lastSeq) this.lastSeq = seq;
        }

        // Route Resync to handlers.
        if (msg.type === 'Resync') {
          const resyncHandlers = this.handlers.get('Resync');
          if (resyncHandlers) {
            for (const handler of resyncHandlers) {
              try { handler(msg); } catch { /* swallow */ }
            }
          }
          return;
        }

        // Route to registered handlers.
        const typeHandlers = this.handlers.get(msg.type);
        if (typeHandlers) {
          for (const handler of typeHandlers) {
            try { handler(msg); } catch { /* swallow */ }
          }
        }
      };
    } catch {
      // BroadcastChannel not supported — single-tab mode.
      return;
    }

    // Use Web Locks API for leader election if available.
    if (typeof navigator !== 'undefined' && navigator.locks) {
      // Request the lock without stealing. Only one tab can hold it.
      // When a leader tab closes, the lock is released and the next
      // waiting tab becomes leader.
      navigator.locks.request('ghost-ws-leader', { ifAvailable: true }, async (lock) => {
        if (lock) {
          // We got the lock — we are the leader.
          this.becomeLeader();
          // Hold the lock forever (until tab closes).
          // Return a promise that never resolves so the lock is held.
          return new Promise<void>(() => {});
        } else {
          // Another tab is already the leader — become follower.
          this.becomeFollower();
          // Wait for the lock (will fire when current leader tab closes).
          navigator.locks.request('ghost-ws-leader', async () => {
            this.becomeLeader();
            return new Promise<void>(() => {});
          });
        }
      });
    }
    // If Web Locks not available, isLeader stays true (default) — single-leader fallback.
  }

  private becomeLeader() {
    if (this.isLeader) return;
    this.isLeader = true;
    // Open a new WS connection as the leader.
    void this.connect();
  }

  private becomeFollower() {
    if (!this.isLeader) return;
    this.isLeader = false;
    this.state = 'connected'; // We're "connected" via BroadcastChannel.
    // Close our own WS — the leader will forward events via BroadcastChannel.
    if (this.ws) {
      this.intentionalClose = true;
      this.ws.close(1000, 'Follower — deferring to leader');
      this.ws = null;
    }
    this.clearReconnectTimer();
  }
}

/** Singleton WebSocket store instance. */
export const wsStore = new WebSocketStore();
