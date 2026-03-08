import { createTimeoutSignal, type GhostClientOptions } from './client.js';

// ── Types ──

/** All possible server-to-client WebSocket events. */
export type WsEvent =
  | {
      type: 'ScoreUpdate';
      agent_id: string;
      score: number;
      level: number;
      signals: number[];
    }
  | {
      type: 'InterventionChange';
      agent_id: string;
      old_level: number;
      new_level: number;
    }
  | {
      type: 'KillSwitchActivation';
      level: string;
      agent_id?: string;
      reason: string;
    }
  | {
      type: 'ProposalDecision';
      proposal_id: string;
      decision: 'approved' | 'rejected';
      agent_id: string;
    }
  | {
      type: 'AgentStateChange';
      agent_id: string;
      new_state: string;
    }
  | {
      type: 'SessionEvent';
      session_id: string;
      event_id: string;
      event_type: string;
      sender?: string;
      sequence_number: number;
    }
  | {
      type: 'ChatMessage';
      session_id: string;
      message_id: string;
      role: 'user' | 'assistant';
      content: string;
      safety_status: 'clean' | 'warning' | 'blocked';
    }
  | { type: 'Ping' }
  | { type: 'Resync'; missed_events: number }
  | { type: string; [key: string]: unknown };

export interface GhostWebSocketOptions {
  /** Topics to subscribe to on connect. */
  topics?: string[];
  /** Auto-reconnect on disconnect. Default: true. */
  autoReconnect?: boolean;
  /** Max reconnect delay in ms. Default: 30000. */
  maxReconnectDelay?: number;
  /** Max reconnect attempts before giving up. Default: 10. Set to 0 for unlimited. */
  maxReconnectAttempts?: number;
  /** Called when reconnection is abandoned after maxReconnectAttempts. */
  onReconnectFailed?: () => void;
  /** Called whenever the normalized websocket envelope is received. */
  onEnvelope?: (envelope: WsEnvelope) => void;
  /** Called when the websocket lifecycle state changes. */
  onStateChange?: (state: GhostWebSocketState) => void;
  /** Called when a reconnect is scheduled. */
  onReconnectScheduled?: (attempt: number, delayMs: number) => void;
  /** Called on websocket transport error. */
  onError?: (message: string) => void;
  /** Optional replay cursor to use before any messages have been received locally. */
  initialLastSeq?: number;
}

type EventHandler = (event: WsEvent) => void;

export interface WsEnvelope {
  seq?: number;
  timestamp?: string;
  event?: WsEvent;
  type?: string;
}

export type GhostWebSocketState =
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'disconnected';

interface WsTicketResponse {
  ticket: string;
}

// ── Implementation ──

export class GhostWebSocket {
  private ws: WebSocket | null = null;
  private handlers = new Map<string, Set<EventHandler>>();
  private globalHandlers = new Set<EventHandler>();
  private subscribedTopics: string[] = [];
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;
  private lastSeq = 0;
  private state: GhostWebSocketState = 'disconnected';
  private connectAttemptId = 0;

  constructor(
    private clientOptions: GhostClientOptions,
    private wsOptions: GhostWebSocketOptions = {},
  ) {
    this.subscribedTopics = wsOptions.topics ?? [];
    this.lastSeq = wsOptions.initialLastSeq ?? 0;
  }

  /** Open the WebSocket connection. */
  connect(): this {
    this.closed = false;
    this.reconnectAttempt = 0;
    this.startConnect();
    return this;
  }

  /** Close the WebSocket connection. */
  disconnect(): void {
    this.closed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
    this.setState('disconnected');
  }

  /** Subscribe to additional topics. */
  subscribe(topics: string[]): void {
    this.subscribedTopics.push(...topics);
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'Subscribe', topics }));
    }
  }

  /** Unsubscribe from topics. */
  unsubscribe(topics: string[]): void {
    this.subscribedTopics = this.subscribedTopics.filter((t) => !topics.includes(t));
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'Unsubscribe', topics }));
    }
  }

  /** Listen for a specific event type. */
  on(eventType: string, handler: EventHandler): () => void {
    let set = this.handlers.get(eventType);
    if (!set) {
      set = new Set();
      this.handlers.set(eventType, set);
    }
    set.add(handler);
    return () => set!.delete(handler);
  }

  /** Listen for all events. */
  onAny(handler: EventHandler): () => void {
    this.globalHandlers.add(handler);
    return () => this.globalHandlers.delete(handler);
  }

  private doConnect(): void {
    const attemptId = ++this.connectAttemptId;
    void this.connectInternal(attemptId);
  }

  private startConnect(): void {
    this.doConnect();
  }

  private async connectInternal(attemptId: number): Promise<void> {
    this.setState('connecting');
    const baseUrl = this.clientOptions.baseUrl ?? 'http://127.0.0.1:39780';
    const wsUrl = baseUrl.replace(/^http/, 'ws') + '/api/ws';
    let protocols: string[] | undefined;

    try {
      protocols = await this.resolveProtocols();
    } catch (error) {
      if (this.closed || attemptId !== this.connectAttemptId) {
        return;
      }

      const message = error instanceof Error ? error.message : 'WebSocket authentication failed';
      this.wsOptions.onError?.(message);
      if (
        error instanceof GhostWebSocketConnectError &&
        error.retryable &&
        !this.closed &&
        (this.wsOptions.autoReconnect ?? true)
      ) {
        this.setState('reconnecting');
        this.scheduleReconnect();
      } else {
        this.closed = true;
        this.setState('disconnected');
      }
      return;
    }

    if (this.closed || attemptId !== this.connectAttemptId) {
      return;
    }

    this.ws = new WebSocket(wsUrl, protocols);

    this.ws.onopen = () => {
      this.reconnectAttempt = 0;
      this.setState('connected');
      if (this.lastSeq > 0) {
        this.ws!.send(JSON.stringify({ last_seq: this.lastSeq }));
      }
      if (this.subscribedTopics.length > 0) {
        this.ws!.send(
          JSON.stringify({ type: 'Subscribe', topics: this.subscribedTopics }),
        );
      }
    };

    this.ws.onmessage = (msg) => {
      try {
        const envelope = this.normalizeEnvelope(String(msg.data));
        if (typeof envelope.seq === 'number' && envelope.seq > this.lastSeq) {
          this.lastSeq = envelope.seq;
        }
        this.wsOptions.onEnvelope?.(envelope);
        const event: WsEvent = envelope.event!;
        // Dispatch to type-specific handlers
        const typeHandlers = this.handlers.get(event.type);
        if (typeHandlers) {
          for (const h of typeHandlers) h(event);
        }
        // Dispatch to global handlers
        for (const h of this.globalHandlers) h(event);
      } catch {
        // Ignore malformed messages
      }
    };

    this.ws.onclose = () => {
      this.ws = null;
      if (!this.closed && (this.wsOptions.autoReconnect ?? true)) {
        this.setState('reconnecting');
        this.scheduleReconnect();
      } else {
        this.setState('disconnected');
      }
    };

    this.ws.onerror = () => {
      this.wsOptions.onError?.('WebSocket connection error');
      // onclose will fire after onerror
    };
  }

  private async resolveProtocols(): Promise<string[] | undefined> {
    if (!this.clientOptions.token) {
      return undefined;
    }

    const baseUrl = this.clientOptions.baseUrl ?? 'http://127.0.0.1:39780';
    const fetchFn = this.clientOptions.fetch ?? globalThis.fetch;
    if (!fetchFn) {
      throw new GhostWebSocketConnectError(
        'WebSocket authentication requires fetch support to mint an upgrade ticket.',
        false,
      );
    }

    let response: Response;
    try {
      response = await fetchFn(`${baseUrl}/api/ws/tickets`, {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${this.clientOptions.token}`,
          Accept: 'application/json',
        },
        signal: createTimeoutSignal(this.clientOptions.timeout),
      });
    } catch (error) {
      if (error instanceof DOMException && error.name === 'TimeoutError') {
        throw new GhostWebSocketConnectError('WebSocket ticket request timed out.', true);
      }
      throw new GhostWebSocketConnectError(
        'Failed to mint a WebSocket upgrade ticket.',
        true,
      );
    }

    if (!response.ok) {
      const message = await response.text().catch(() => '');
      const reason = message || `HTTP ${response.status}`;
      throw new GhostWebSocketConnectError(
        `WebSocket authentication failed: ${reason}`,
        !(response.status === 401 || response.status === 403),
      );
    }

    const body = await response.json() as WsTicketResponse;
    if (!body.ticket) {
      throw new GhostWebSocketConnectError(
        'WebSocket ticket response did not include a ticket.',
        false,
      );
    }

    return [`ghost-ticket.${body.ticket}`];
  }

  private scheduleReconnect(): void {
    const maxAttempts = this.wsOptions.maxReconnectAttempts ?? 10;
    if (maxAttempts > 0 && this.reconnectAttempt >= maxAttempts) {
      this.closed = true;
      this.wsOptions.onReconnectFailed?.();
      return;
    }
    const maxDelay = this.wsOptions.maxReconnectDelay ?? 30_000;
    const delay = Math.min(1000 * 2 ** this.reconnectAttempt, maxDelay);
    this.reconnectAttempt++;
    this.wsOptions.onReconnectScheduled?.(this.reconnectAttempt, delay);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.startConnect();
    }, delay);
  }

  private normalizeEnvelope(raw: string): WsEnvelope {
    const parsed = JSON.parse(raw) as WsEnvelope;
    return parsed.event ? parsed : { event: parsed as WsEvent };
  }

  private setState(state: GhostWebSocketState): void {
    this.state = state;
    this.wsOptions.onStateChange?.(state);
  }
}

class GhostWebSocketConnectError extends Error {
  constructor(
    message: string,
    readonly retryable: boolean,
  ) {
    super(message);
  }
}
