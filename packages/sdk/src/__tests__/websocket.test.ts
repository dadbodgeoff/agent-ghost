import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GhostWebSocket } from '../websocket.js';

class MockWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  static instances: MockWebSocket[] = [];

  readonly url: string;
  readonly protocols?: string | string[];
  readyState = MockWebSocket.CONNECTING;
  sent: string[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;

  constructor(url: string, protocols?: string | string[]) {
    this.url = url;
    this.protocols = protocols;
    MockWebSocket.instances.push(this);
  }

  send(data: string) {
    this.sent.push(data);
  }

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.();
  }

  open() {
    this.readyState = MockWebSocket.OPEN;
    this.onopen?.();
  }

  emitMessage(data: string) {
    this.onmessage?.({ data });
  }
}

describe('GhostWebSocket', () => {
  beforeEach(() => {
    MockWebSocket.instances = [];
    vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('uses ghost-token subprotocols and sends initial subscriptions', () => {
    const socket = new GhostWebSocket(
      { baseUrl: 'http://test:39780', token: 'secret-token' },
      { topics: ['agent:alpha'] },
    ).connect();

    const transport = MockWebSocket.instances[0];
    expect(transport.url).toBe('ws://test:39780/api/ws');
    expect(transport.protocols).toEqual(['ghost-token.secret-token']);

    transport.open();

    expect(transport.sent).toEqual([
      JSON.stringify({ type: 'Subscribe', topics: ['agent:alpha'] }),
    ]);

    socket.disconnect();
  });

  it('reconnects with last_seq before replaying subscriptions', () => {
    vi.useFakeTimers();

    const socket = new GhostWebSocket(
      { baseUrl: 'http://test:39780', token: 'secret-token' },
      { topics: ['agent:alpha'] },
    ).connect();

    const firstTransport = MockWebSocket.instances[0];
    firstTransport.open();
    firstTransport.emitMessage(
      JSON.stringify({
        seq: 7,
        timestamp: '2026-03-07T12:00:00Z',
        event: { type: 'Ping' },
      }),
    );
    firstTransport.close();

    vi.advanceTimersByTime(1000);

    const secondTransport = MockWebSocket.instances[1];
    expect(secondTransport).toBeDefined();

    secondTransport.open();

    expect(secondTransport.sent).toEqual([
      JSON.stringify({ last_seq: 7 }),
      JSON.stringify({ type: 'Subscribe', topics: ['agent:alpha'] }),
    ]);

    socket.disconnect();
  });

  it('ignores malformed websocket payloads', () => {
    const handler = vi.fn();
    const socket = new GhostWebSocket({ baseUrl: 'http://test:39780' });
    socket.onAny(handler);
    socket.connect();

    const transport = MockWebSocket.instances[0];
    transport.open();
    transport.emitMessage('{not-json');

    expect(handler).not.toHaveBeenCalled();

    socket.disconnect();
  });

  it('reports lifecycle callbacks and reuses an initial replay cursor', () => {
    vi.useFakeTimers();

    const states: string[] = [];
    const reconnectAttempts: Array<{ attempt: number; delayMs: number }> = [];
    const envelopes: Array<{ seq?: number; type?: string }> = [];

    const socket = new GhostWebSocket(
      { baseUrl: 'http://test:39780', token: 'secret-token' },
      {
        initialLastSeq: 12,
        onStateChange: (state) => states.push(state),
        onReconnectScheduled: (attempt, delayMs) => reconnectAttempts.push({ attempt, delayMs }),
        onEnvelope: (envelope) => envelopes.push({ seq: envelope.seq, type: envelope.event?.type }),
      },
    ).connect();

    const firstTransport = MockWebSocket.instances[0];
    expect(states).toEqual(['connecting']);

    firstTransport.open();
    expect(firstTransport.sent).toEqual([JSON.stringify({ last_seq: 12 })]);
    expect(states).toEqual(['connecting', 'connected']);

    firstTransport.emitMessage(
      JSON.stringify({
        seq: 13,
        timestamp: '2026-03-07T12:00:00Z',
        event: { type: 'Ping' },
      }),
    );
    expect(envelopes).toEqual([{ seq: 13, type: 'Ping' }]);

    firstTransport.close();
    expect(states.at(-1)).toBe('reconnecting');
    expect(reconnectAttempts).toEqual([{ attempt: 1, delayMs: 1000 }]);

    vi.advanceTimersByTime(1000);

    const secondTransport = MockWebSocket.instances[1];
    secondTransport.open();
    expect(secondTransport.sent).toEqual([JSON.stringify({ last_seq: 13 })]);

    socket.disconnect();
  });

  it('normalizes flat websocket events into envelopes', () => {
    const handler = vi.fn();
    const envelopes: Array<{ type?: string }> = [];
    const socket = new GhostWebSocket(
      { baseUrl: 'http://test:39780' },
      {
        onEnvelope: (envelope) => envelopes.push({ type: envelope.event?.type }),
      },
    );
    socket.onAny(handler);
    socket.connect();

    const transport = MockWebSocket.instances[0];
    transport.open();
    transport.emitMessage(JSON.stringify({ type: 'Ping' }));

    expect(handler).toHaveBeenCalledWith({ type: 'Ping' });
    expect(envelopes).toEqual([{ type: 'Ping' }]);

    socket.disconnect();
  });
});
