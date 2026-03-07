import { GhostAPIError, GhostNetworkError, GhostTimeoutError } from './errors.js';
import { AgentsAPI } from './agents.js';
import { SessionsAPI } from './sessions.js';
import { ChatAPI } from './chat.js';
import { ConvergenceAPI } from './convergence.js';
import { GoalsAPI } from './goals.js';
import { SkillsAPI } from './skills.js';
import { SafetyAPI } from './safety.js';
import { HealthAPI } from './health.js';
import { AuthAPI } from './auth.js';
import { AuditAPI } from './audit.js';
import { CostsAPI } from './costs.js';
import { MemoryAPI } from './memory.js';
import { RuntimeSessionsAPI } from './runtime-sessions.js';
import { TracesAPI } from './traces.js';
import { GhostWebSocket, type GhostWebSocketOptions } from './websocket.js';

// ── Types ──

export interface GhostClientOptions {
  /** Base URL of the Ghost gateway. Default: http://127.0.0.1:39780 */
  baseUrl?: string;
  /** Bearer token for authentication. */
  token?: string;
  /** Custom fetch implementation (for testing, SSR, or polyfilling). */
  fetch?: typeof globalThis.fetch;
  /** Default request timeout in milliseconds. */
  timeout?: number;
}

/** Internal request function signature shared with API modules. */
export type GhostRequestFn = <T>(
  method: string,
  path: string,
  body?: unknown,
) => Promise<T>;

// ── Client ──

export class GhostClient {
  readonly agents: AgentsAPI;
  readonly sessions: SessionsAPI;
  readonly chat: ChatAPI;
  readonly convergence: ConvergenceAPI;
  readonly goals: GoalsAPI;
  readonly skills: SkillsAPI;
  readonly safety: SafetyAPI;
  readonly health: HealthAPI;
  readonly auth: AuthAPI;
  readonly audit: AuditAPI;
  readonly costs: CostsAPI;
  readonly memory: MemoryAPI;
  readonly runtimeSessions: RuntimeSessionsAPI;
  readonly traces: TracesAPI;

  private readonly options: GhostClientOptions;

  constructor(options?: GhostClientOptions) {
    this.options = {
      baseUrl: 'http://127.0.0.1:39780',
      ...options,
    };

    const request = this.request.bind(this);
    this.agents = new AgentsAPI(request);
    this.sessions = new SessionsAPI(request);
    this.chat = new ChatAPI(request, this.options);
    this.convergence = new ConvergenceAPI(request);
    this.goals = new GoalsAPI(request);
    this.skills = new SkillsAPI(request);
    this.safety = new SafetyAPI(request);
    this.health = new HealthAPI(request);
    this.auth = new AuthAPI(request);
    this.audit = new AuditAPI(request, this.options);
    this.costs = new CostsAPI(request);
    this.memory = new MemoryAPI(request);
    this.runtimeSessions = new RuntimeSessionsAPI(request);
    this.traces = new TracesAPI(request);
  }

  /** Create a WebSocket connection for real-time events. */
  ws(options?: GhostWebSocketOptions): GhostWebSocket {
    return new GhostWebSocket(this.options, options);
  }

  /** Internal request method used by all API modules. */
  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const url = `${this.options.baseUrl}${path}`;
    const fetchFn = this.options.fetch ?? globalThis.fetch;

    const headers: Record<string, string> = {
      Accept: 'application/json',
    };

    if (body !== undefined) {
      headers['Content-Type'] = 'application/json';
    }

    if (this.options.token) {
      headers['Authorization'] = `Bearer ${this.options.token}`;
    }

    let response: Response;
    try {
      response = await fetchFn(url, {
        method,
        headers,
        body: body !== undefined ? JSON.stringify(body) : undefined,
        signal: this.options.timeout
          ? AbortSignal.timeout(this.options.timeout)
          : undefined,
      });
    } catch (err) {
      if (err instanceof DOMException && err.name === 'TimeoutError') {
        throw new GhostTimeoutError(this.options.timeout!);
      }
      throw new GhostNetworkError(
        `Failed to connect to Ghost API at ${this.options.baseUrl}`,
        err instanceof Error ? err : undefined,
      );
    }

    if (!response.ok) {
      let errorMessage = `HTTP ${response.status}`;
      let errorCode: string | undefined;
      let errorDetails: Record<string, unknown> | undefined;

      try {
        const text = await response.text();
        if (text && text.trim().length > 0) {
          try {
            const errorBody = JSON.parse(text);
            if (typeof errorBody === 'object' && errorBody !== null) {
              if ('error' in errorBody) {
                if (typeof errorBody.error === 'string') {
                  errorMessage = errorBody.error;
                } else if (typeof errorBody.error === 'object' && errorBody.error !== null) {
                  const e = errorBody.error as Record<string, unknown>;
                  errorMessage = (e.message as string) ?? errorMessage;
                  errorCode = e.code as string | undefined;
                  errorDetails = e.details as Record<string, unknown> | undefined;
                }
              }
            }
          } catch {
            // Non-JSON error body — use the raw text
            errorMessage = text.substring(0, 500);
          }
        }
      } catch {
        // Could not read response body
      }

      throw new GhostAPIError(errorMessage, response.status, errorCode, errorDetails);
    }

    // Handle empty responses (204 No Content, or zero-length body)
    const contentLength = response.headers.get('content-length');
    const contentType = response.headers.get('content-type') ?? '';

    if (
      response.status === 204 ||
      contentLength === '0' ||
      !contentType.includes('application/json')
    ) {
      // Try to parse as JSON anyway if there might be a body,
      // but fall back to undefined for truly empty responses.
      if (contentLength === '0' || response.status === 204) {
        return undefined as T;
      }
      // Non-JSON content type with a body — read as text and attempt parse
      const text = await response.text();
      if (!text || text.trim().length === 0) {
        return undefined as T;
      }
      try {
        return JSON.parse(text) as T;
      } catch {
        return undefined as T;
      }
    }

    return response.json() as Promise<T>;
  }
}
