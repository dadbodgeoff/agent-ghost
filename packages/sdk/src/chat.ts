import type {
  GhostRequestFn,
  GhostClientOptions,
  GhostRequestOptions,
} from './client.js';
import { resolveGhostOperationEnvelope } from './client.js';
import type { components, operations } from './generated-types.js';
import type { StudioMessage, StudioSafetyStatus } from './sessions.js';
import { GhostAPIError, GhostNetworkError } from './errors.js';

// ── Types ──

export type SendMessageParams =
  operations['send_studio_message']['requestBody']['content']['application/json'];

type SendMessageCompletedResult = Omit<
  components['schemas']['StudioSendMessageResponseSchema'],
  'assistant_message' | 'safety_status' | 'user_message'
> & {
  user_message: StudioMessage;
  assistant_message: StudioMessage;
  safety_status: StudioSafetyStatus;
};
type SendMessageAcceptedResult = components['schemas']['StudioMessageAcceptedResponseSchema'];
export type SendMessageResult = SendMessageCompletedResult | SendMessageAcceptedResult;

export type StreamErrorType =
  | 'provider_unavailable'
  | 'auth_failed'
  | 'runtime_error';

export type StreamWarningType = 'persistence_degraded';

/**
 * Explicit exception record: Studio SSE payloads are not generated from OpenAPI today,
 * so the live stream contract remains hand-maintained here.
 */
export type StreamEvent =
  | { type: 'stream_start'; message_id: string; session_id?: string; reconstructed?: boolean }
  | { type: 'text_delta'; content: string; reconstructed?: boolean }
  | { type: 'tool_use'; tool: string; tool_id: string; status: string }
  | { type: 'tool_result'; tool: string; tool_id: string; status: string; preview?: string }
  | { type: 'heartbeat'; phase: string }
  | {
      type: 'stream_end';
      message_id: string;
      token_count: number;
      safety_status: 'clean' | 'warning' | 'blocked';
      reconstructed?: boolean;
    }
  | {
      type: 'error';
      message: string;
      error_type?: StreamErrorType;
      provider?: string;
      fallback?: boolean;
      terminal?: boolean;
      reconstructed?: boolean;
    }
  | {
      type: 'warning';
      warning_type: StreamWarningType;
      code?: string;
      message: string;
    };

export type ChatStreamEventHandler = (
  eventType: string,
  data: Record<string, unknown>,
  eventId?: string,
) => void;

// ── API ──

export class ChatAPI {
  constructor(
    private request: GhostRequestFn,
    private options: GhostClientOptions,
  ) {}

  private buildStreamHeaders(
    envelope: ReturnType<typeof resolveGhostOperationEnvelope>,
  ): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      Accept: 'text/event-stream',
      'X-Ghost-Client-Name': this.options.clientName ?? 'sdk',
      'X-Ghost-Client-Version': this.options.clientVersion ?? '0.1.0',
    };

    if (this.options.token) {
      headers['Authorization'] = `Bearer ${this.options.token}`;
    }
    if (envelope.requestId) {
      headers['X-Request-ID'] = envelope.requestId;
    }
    if (envelope.operationId) {
      headers['X-Ghost-Operation-ID'] = envelope.operationId;
    }
    if (envelope.idempotencyKey) {
      headers['Idempotency-Key'] = envelope.idempotencyKey;
    }

    return headers;
  }

  /** Send a message over the blocking Studio path. Returns either completion or recovery metadata. */
  async send(
    sessionId: string,
    params: SendMessageParams,
    options?: GhostRequestOptions,
  ): Promise<SendMessageResult> {
    return this.request<SendMessageResult>(
      'POST',
      `/api/studio/sessions/${encodeURIComponent(sessionId)}/messages`,
      params,
      options,
    );
  }

  /** Send a message and receive streaming SSE events. */
  async *stream(
    sessionId: string,
    params: SendMessageParams,
    options?: GhostRequestOptions,
  ): AsyncGenerator<StreamEvent> {
    const baseUrl = this.options.baseUrl ?? 'http://127.0.0.1:39780';
    const url = `${baseUrl}/api/studio/sessions/${encodeURIComponent(sessionId)}/messages/stream`;
    const envelope = resolveGhostOperationEnvelope('POST', options);
    const headers = this.buildStreamHeaders(envelope);

    const fetchFn = this.options.fetch ?? globalThis.fetch;

    let response: Response;
    try {
      response = await fetchFn(url, {
        method: 'POST',
        headers,
        body: JSON.stringify(params),
        signal: this.options.timeout
          ? AbortSignal.timeout(this.options.timeout)
          : undefined,
      });
    } catch (err) {
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${baseUrl}`,
        err instanceof Error ? err : undefined,
      );
    }

    if (!response.ok) {
      const text = await response.text().catch(() => '');
      throw new GhostAPIError(
        text || `HTTP ${response.status}`,
        response.status,
      );
    }

    if (!response.body) {
      throw new GhostNetworkError('Response body is null — streaming not supported in this environment');
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() ?? '';

        let eventType = '';
        let dataLines: string[] = [];

        for (const line of lines) {
          if (line.startsWith('event: ')) {
            eventType = line.slice(7).trim();
          } else if (line.startsWith('data: ')) {
            dataLines.push(line.slice(6));
          } else if (line.startsWith('data:')) {
            // "data:" with no space — value is rest of line (SSE spec)
            dataLines.push(line.slice(5));
          } else if (line === '' && dataLines.length > 0) {
            // Empty line = end of event. Join multi-line data with newlines (SSE spec).
            const eventData = dataLines.join('\n');
            try {
              const parsed = JSON.parse(eventData);
              yield { type: eventType || parsed.type, ...parsed } as StreamEvent;
            } catch {
              // Skip malformed events
            }
            eventType = '';
            dataLines = [];
          } else if (line === '') {
            // Empty line but no data — reset
            eventType = '';
            dataLines = [];
          }
        }
      }
    } finally {
      reader.releaseLock();
    }
  }

  async streamWithCallback(
    sessionId: string,
    params: SendMessageParams,
    onEvent: ChatStreamEventHandler,
    signal?: AbortSignal,
    options?: GhostRequestOptions,
  ): Promise<void> {
    const baseUrl = this.options.baseUrl ?? 'http://127.0.0.1:39780';
    const url = `${baseUrl}/api/studio/sessions/${encodeURIComponent(sessionId)}/messages/stream`;
    const envelope = resolveGhostOperationEnvelope('POST', options);
    const headers = this.buildStreamHeaders(envelope);

    const fetchFn = this.options.fetch ?? globalThis.fetch;

    let response: Response;
    try {
      response = await fetchFn(url, {
        method: 'POST',
        headers,
        body: JSON.stringify(params),
        signal,
      });
    } catch (err) {
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${baseUrl}`,
        err instanceof Error ? err : undefined,
      );
    }

    if (!response.ok) {
      const text = await response.text().catch(() => '');
      throw new GhostAPIError(text || `HTTP ${response.status}`, response.status);
    }

    if (!response.body) {
      throw new GhostNetworkError('Response body is null — streaming not supported in this environment');
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let aborted = false;

    const onAbort = () => {
      aborted = true;
      reader.cancel().catch(() => {});
    };
    signal?.addEventListener('abort', onAbort);

    try {
      while (!aborted) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const events = buffer.split('\n\n');
        buffer = events.pop() || '';

        for (const eventBlock of events) {
          if (!eventBlock.trim()) continue;

          const lines = eventBlock.split('\n');
          let eventType = 'message';
          let eventId: string | undefined;
          const dataLines: string[] = [];

          for (const line of lines) {
            if (line.startsWith('event: ')) {
              eventType = line.slice(7).trim();
            } else if (line.startsWith('data: ')) {
              dataLines.push(line.slice(6));
            } else if (line.startsWith('id: ')) {
              eventId = line.slice(4).trim();
            }
          }

          if (dataLines.length === 0) continue;

          const dataStr = dataLines.join('\n');
          try {
            const data = JSON.parse(dataStr) as Record<string, unknown>;
            onEvent(eventType, data, eventId);
          } catch {
            onEvent(eventType, { message: dataStr }, eventId);
          }
        }
      }
    } finally {
      signal?.removeEventListener('abort', onAbort);
      reader.releaseLock();
    }
  }
}
