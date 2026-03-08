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
import { SearchAPI } from './search.js';
import { TracesAPI } from './traces.js';
import { WorkflowsAPI } from './workflows.js';
import { ProfilesAPI } from './profiles.js';
import { WebhooksAPI } from './webhooks.js';
import { BackupsAPI } from './backups.js';
import { ProviderKeysAPI } from './provider-keys.js';
import { PushAPI } from './push.js';
import { ChannelsAPI } from './channels.js';
import { StateAPI } from './state.js';
import { IntegrityAPI } from './integrity.js';
import { MeshAPI } from './mesh.js';
import { CompatibilityAPI, type GhostClientIdentity } from './compatibility.js';
import { A2AAPI } from './a2a.js';
import { StudioAPI } from './studio.js';
import { PcControlAPI } from './pc-control.js';
import { OAuthAPI } from './oauth.js';
import { ItpAPI } from './itp.js';
import { GhostWebSocket, type GhostWebSocketOptions } from './websocket.js';

// ── Types ──

export interface GhostClientOptions {
  /** Base URL of the Ghost gateway. Default: http://127.0.0.1:39780 */
  baseUrl?: string;
  /** Bearer token for authentication. */
  token?: string;
  /** Logical client identity sent to the gateway for compatibility enforcement. */
  clientName?: string;
  /** Semantic version of the calling client. */
  clientVersion?: string;
  /** Custom fetch implementation (for testing, SSR, or polyfilling). */
  fetch?: typeof globalThis.fetch;
  /** Default request timeout in milliseconds. */
  timeout?: number;
}

export interface GhostRequestOptions {
  requestId?: string;
  operationId?: string;
  idempotencyKey?: string;
  idempotency?: 'required' | 'optional' | 'disabled';
}

/** Internal request function signature shared with API modules. */
export type GhostRequestFn = <T>(
  method: string,
  path: string,
  body?: unknown,
  options?: GhostRequestOptions,
) => Promise<T>;

export interface GhostOperationEnvelope {
  requestId?: string;
  operationId?: string;
  idempotencyKey?: string;
}

const MUTATING_METHODS = new Set(['POST', 'PUT', 'PATCH', 'DELETE']);
const SAFE_RETRY_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);
const RETRYABLE_STATUS_CODES = new Set([408, 429, 500, 502, 503, 504]);
const MAX_RETRY_ATTEMPTS = 2;
const RETRY_BASE_DELAY_MS = 150;
const RETRY_MAX_DELAY_MS = 1500;
const DEFAULT_CLIENT_NAME = 'sdk';
const DEFAULT_CLIENT_VERSION = '0.1.0';

function resolveGhostClientIdentity(options: GhostClientOptions): GhostClientIdentity {
  return {
    name: options.clientName ?? DEFAULT_CLIENT_NAME,
    version: options.clientVersion ?? DEFAULT_CLIENT_VERSION,
  };
}

function isMutatingMethod(method: string): boolean {
  return MUTATING_METHODS.has(method.toUpperCase());
}

function secureCrypto(): Crypto {
  if (!globalThis.crypto?.getRandomValues || !globalThis.crypto?.randomUUID) {
    throw new Error(
      'Ghost SDK requires Web Crypto for generated request and operation IDs. ' +
      'Provide explicit IDs or run in an environment with crypto.getRandomValues().',
    );
  }
  return globalThis.crypto;
}

function formatUuid(bytes: Uint8Array): string {
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20, 32),
  ].join('-');
}

export function generateGhostRequestId(): string {
  return secureCrypto().randomUUID();
}

export function generateGhostOperationId(): string {
  const bytes = new Uint8Array(16);
  secureCrypto().getRandomValues(bytes);
  const timestamp = BigInt(Date.now());

  bytes[0] = Number((timestamp >> 40n) & 0xffn);
  bytes[1] = Number((timestamp >> 32n) & 0xffn);
  bytes[2] = Number((timestamp >> 24n) & 0xffn);
  bytes[3] = Number((timestamp >> 16n) & 0xffn);
  bytes[4] = Number((timestamp >> 8n) & 0xffn);
  bytes[5] = Number(timestamp & 0xffn);
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;

  return formatUuid(bytes);
}

export function resolveGhostOperationEnvelope(
  method: string,
  options?: GhostRequestOptions,
): GhostOperationEnvelope {
  const mutating = isMutatingMethod(method);
  const idempotencyEnabled =
    options?.idempotency === 'required' ||
    (options?.idempotency !== 'disabled' && mutating);

  const operationId =
    options?.operationId ??
    ((mutating || options?.idempotency === 'required') ? generateGhostOperationId() : undefined);

  const idempotencyKey =
    options?.idempotencyKey ??
    (idempotencyEnabled ? operationId : undefined);

  const requestId =
    options?.requestId ??
    ((mutating || operationId !== undefined || idempotencyKey !== undefined)
      ? generateGhostRequestId()
      : undefined);

  return {
    requestId,
    operationId,
    idempotencyKey,
  };
}

export function createTimeoutSignal(timeout?: number): AbortSignal | undefined {
  return timeout ? AbortSignal.timeout(timeout) : undefined;
}

function isRetryableRequest(method: string, options?: GhostRequestOptions): boolean {
  const normalized = method.toUpperCase();
  if (SAFE_RETRY_METHODS.has(normalized)) {
    return true;
  }
  return options?.idempotency === 'required';
}

function isRetryableResponse(response: Response): boolean {
  return RETRYABLE_STATUS_CODES.has(response.status);
}

function parseRetryAfterMs(response: Response): number | undefined {
  const retryAfter = response.headers.get('retry-after');
  if (!retryAfter) {
    return undefined;
  }
  const seconds = Number(retryAfter);
  if (Number.isFinite(seconds) && seconds >= 0) {
    return seconds * 1000;
  }
  const timestamp = Date.parse(retryAfter);
  if (!Number.isNaN(timestamp)) {
    return Math.max(0, timestamp - Date.now());
  }
  return undefined;
}

function computeBackoffDelayMs(attempt: number, response?: Response): number {
  const retryAfterMs = response ? parseRetryAfterMs(response) : undefined;
  if (retryAfterMs !== undefined) {
    return Math.min(retryAfterMs, RETRY_MAX_DELAY_MS);
  }

  const cap = Math.min(RETRY_BASE_DELAY_MS * 2 ** attempt, RETRY_MAX_DELAY_MS);
  const baseDelay = Math.floor(cap / 2);
  if (!globalThis.crypto?.getRandomValues) {
    return baseDelay;
  }

  const jitterRange = Math.max(1, cap - baseDelay);
  const jitterSource = globalThis.crypto.getRandomValues(new Uint32Array(1))[0];
  return Math.min(cap, baseDelay + (jitterSource % jitterRange));
}

async function sleep(delayMs: number): Promise<void> {
  await new Promise((resolve) => {
    setTimeout(resolve, delayMs);
  });
}

async function parseGhostError(response: Response): Promise<GhostAPIError> {
  let errorMessage = `HTTP ${response.status}`;
  let errorCode: string | undefined;
  let errorDetails: Record<string, unknown> | undefined;

  try {
    const text = await response.text();
    if (text && text.trim().length > 0) {
      try {
        const errorBody = JSON.parse(text);
        if (typeof errorBody === 'object' && errorBody !== null && 'error' in errorBody) {
          if (typeof errorBody.error === 'string') {
            errorMessage = errorBody.error;
          } else if (typeof errorBody.error === 'object' && errorBody.error !== null) {
            const error = errorBody.error as Record<string, unknown>;
            errorMessage = (error.message as string) ?? errorMessage;
            errorCode = error.code as string | undefined;
            errorDetails = error.details as Record<string, unknown> | undefined;
          }
        }
      } catch {
        errorMessage = text.substring(0, 500);
      }
    }
  } catch {
    // Ignore response body parsing failures and return the status-derived error.
  }

  return new GhostAPIError(errorMessage, response.status, errorCode, errorDetails);
}

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
  readonly search: SearchAPI;
  readonly traces: TracesAPI;
  readonly workflows: WorkflowsAPI;
  readonly profiles: ProfilesAPI;
  readonly webhooks: WebhooksAPI;
  readonly backups: BackupsAPI;
  readonly providerKeys: ProviderKeysAPI;
  readonly push: PushAPI;
  readonly channels: ChannelsAPI;
  readonly state: StateAPI;
  readonly integrity: IntegrityAPI;
  readonly compatibility: CompatibilityAPI;
  readonly mesh: MeshAPI;
  readonly a2a: A2AAPI;
  readonly studio: StudioAPI;
  readonly pcControl: PcControlAPI;
  readonly oauth: OAuthAPI;
  readonly itp: ItpAPI;

  private readonly options: GhostClientOptions;

  constructor(options?: GhostClientOptions) {
    this.options = {
      baseUrl: 'http://127.0.0.1:39780',
      clientName: DEFAULT_CLIENT_NAME,
      clientVersion: DEFAULT_CLIENT_VERSION,
      ...options,
    };

    const request = this.request.bind(this);
    const clientIdentity = resolveGhostClientIdentity(this.options);
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
    this.search = new SearchAPI(request);
    this.traces = new TracesAPI(request);
    this.workflows = new WorkflowsAPI(request);
    this.profiles = new ProfilesAPI(request);
    this.webhooks = new WebhooksAPI(request);
    this.backups = new BackupsAPI(request);
    this.providerKeys = new ProviderKeysAPI(request);
    this.push = new PushAPI(request);
    this.channels = new ChannelsAPI(request);
    this.state = new StateAPI(request);
    this.integrity = new IntegrityAPI(request);
    this.compatibility = new CompatibilityAPI(request, clientIdentity);
    this.mesh = new MeshAPI(request);
    this.a2a = new A2AAPI(request);
    this.studio = new StudioAPI(request);
    this.pcControl = new PcControlAPI(request);
    this.oauth = new OAuthAPI(request);
    this.itp = new ItpAPI(request);
  }

  /** Create a WebSocket connection for real-time events. */
  ws(options?: GhostWebSocketOptions): GhostWebSocket {
    return new GhostWebSocket(this.options, options);
  }

  /** Internal request method used by all API modules. */
  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    options?: GhostRequestOptions,
  ): Promise<T> {
    const url = `${this.options.baseUrl}${path}`;
    const fetchFn = this.options.fetch ?? globalThis.fetch;
    const envelope = resolveGhostOperationEnvelope(method, options);

    const headers: Record<string, string> = {
      Accept: 'application/json',
      'X-Ghost-Client-Name': this.options.clientName!,
      'X-Ghost-Client-Version': this.options.clientVersion!,
    };

    if (body !== undefined) {
      headers['Content-Type'] = 'application/json';
    }

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

    const retryable = isRetryableRequest(method, options);
    let response: Response | undefined;
    let lastError: Error | undefined;

    for (let attempt = 0; attempt <= MAX_RETRY_ATTEMPTS; attempt += 1) {
      try {
        response = await fetchFn(url, {
          method,
          headers,
          body: body !== undefined ? JSON.stringify(body) : undefined,
          signal: createTimeoutSignal(this.options.timeout),
        });
      } catch (err) {
        if (err instanceof DOMException && err.name === 'TimeoutError') {
          lastError = new GhostTimeoutError(this.options.timeout!);
        } else {
          lastError = new GhostNetworkError(
            `Failed to connect to Ghost API at ${this.options.baseUrl}`,
            err instanceof Error ? err : undefined,
          );
        }

        if (!retryable || attempt === MAX_RETRY_ATTEMPTS) {
          throw lastError;
        }
        await sleep(computeBackoffDelayMs(attempt));
        continue;
      }

      if (response.ok || !retryable || !isRetryableResponse(response) || attempt === MAX_RETRY_ATTEMPTS) {
        break;
      }

      await sleep(computeBackoffDelayMs(attempt, response));
    }

    if (!response) {
      throw lastError ?? new GhostNetworkError(`Failed to connect to Ghost API at ${this.options.baseUrl}`);
    }

    if (!response.ok) {
      throw await parseGhostError(response);
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
