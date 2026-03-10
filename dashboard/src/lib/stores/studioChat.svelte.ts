/**
 * Studio chat session store — Svelte 5 runes.
 *
 * DB-backed session persistence via the SDK-backed API.
 * Supports SSE streaming for real-time token display.
 * Persists activeSessionId to localStorage for cross-navigation restore.
 */

import { getGhostClient } from '$lib/ghost-client';
import { wsStore } from '$lib/stores/websocket.svelte';
import type {
  RecoverStreamResult,
  StreamEvent,
  StudioSession as ApiStudioSession,
  StudioSessionWithMessages,
} from '@ghost/sdk';

export interface StudioSession {
  id: string;
  title: string;
  model: string;
  system_prompt: string;
  temperature: number;
  max_tokens: number;
  created_at: string;
  updated_at: string;
  messages: StudioMessage[];
}

export interface ToolCallEntry {
  tool: string;
  toolId: string;
  status: 'running' | 'done' | 'error';
  preview?: string;
}

export interface StudioMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  token_count: number;
  safety_status: 'clean' | 'warning' | 'blocked';
  created_at: string;
  toolCalls?: ToolCallEntry[];
  /** Stream completion status. Absent or 'complete' = normal. */
  status?: 'complete' | 'incomplete' | 'error';
}

const STORAGE_KEY = 'ghost-studio-active-session';
type StreamStartEvent = Extract<StreamEvent, { type: 'stream_start' }>;
type TextDeltaEvent = Extract<StreamEvent, { type: 'text_delta' }>;
type ToolUseEvent = Extract<StreamEvent, { type: 'tool_use' }>;
type ToolResultEvent = Extract<StreamEvent, { type: 'tool_result' }>;
type StreamEndEvent = Extract<StreamEvent, { type: 'stream_end' }>;
type ErrorEvent = Extract<StreamEvent, { type: 'error' }>;
type WarningEvent = Extract<StreamEvent, { type: 'warning' }>;
type SafetyStatus = StudioMessage['safety_status'];

interface RecoverTextChunkPayload {
  content: string;
  reconstructed?: boolean;
}

interface RecoverToolUsePayload {
  tool: string;
  tool_id: string;
}

interface RecoverToolResultPayload extends RecoverToolUsePayload {
  status: string;
  preview?: string;
}

interface RecoverTurnCompletePayload {
  token_count?: number;
  safety_status?: SafetyStatus;
  reconstructed?: boolean;
}

function upsertToolCall(
  calls: ToolCallEntry[] | undefined,
  nextCall: ToolCallEntry,
): ToolCallEntry[] {
  const existing = [...(calls ?? [])];
  const index = existing.findIndex((call) => call.toolId === nextCall.toolId);
  if (index >= 0) {
    existing[index] = { ...existing[index], ...nextCall };
    return existing;
  }
  existing.push(nextCall);
  return existing;
}

const RECONSTRUCTED_REPLAY_WARNING =
  'Recovered final output from the stored assistant message; exact stream replay was unavailable.';

function toStudioSession(session: ApiStudioSession, messages: StudioMessage[] = []): StudioSession {
  return { ...session, messages };
}

function readString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function readNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function readBoolean(value: unknown): boolean | null {
  return typeof value === 'boolean' ? value : null;
}

function readSafetyStatus(value: unknown): SafetyStatus | null {
  return value === 'clean' || value === 'warning' || value === 'blocked' ? value : null;
}

function parseStreamStartEvent(data: Record<string, unknown>): StreamStartEvent | null {
  const message_id = readString(data.message_id);
  if (!message_id) return null;
  return {
    type: 'stream_start',
    execution_id: readString(data.execution_id) ?? undefined,
    message_id,
    session_id: readString(data.session_id) ?? undefined,
    reconstructed: readBoolean(data.reconstructed) ?? undefined,
  };
}

function parseTextDeltaEvent(data: Record<string, unknown>): TextDeltaEvent | null {
  const content = readString(data.content);
  return content !== null
    ? {
        type: 'text_delta',
        content,
        reconstructed: readBoolean(data.reconstructed) ?? undefined,
      }
    : null;
}

function parseToolUseEvent(data: Record<string, unknown>): ToolUseEvent | null {
  const tool = readString(data.tool);
  const tool_id = readString(data.tool_id);
  const status = readString(data.status);
  if (!tool || !tool_id || !status) return null;
  return { type: 'tool_use', tool, tool_id, status };
}

function parseToolResultEvent(data: Record<string, unknown>): ToolResultEvent | null {
  const tool = readString(data.tool);
  const tool_id = readString(data.tool_id);
  const status = readString(data.status);
  if (!tool || !tool_id || !status) return null;
  const preview = readString(data.preview) ?? undefined;
  return { type: 'tool_result', tool, tool_id, status, preview };
}

function parseStreamEndEvent(data: Record<string, unknown>): StreamEndEvent | null {
  const message_id = readString(data.message_id);
  const token_count = readNumber(data.token_count);
  const safety_status = readSafetyStatus(data.safety_status);
  if (!message_id || token_count === null || !safety_status) return null;
  return {
    type: 'stream_end',
    message_id,
    token_count,
    safety_status,
    reconstructed: readBoolean(data.reconstructed) ?? undefined,
  };
}

function parseErrorEvent(data: Record<string, unknown>): ErrorEvent | null {
  const message = readString(data.message);
  if (message === null) return null;

  const error_type = readString(data.error_type);
  return {
    type: 'error',
    message,
    error_type:
      error_type === 'provider_unavailable' ||
      error_type === 'auth_failed' ||
      error_type === 'runtime_error'
        ? error_type
        : undefined,
    provider: readString(data.provider) ?? undefined,
    fallback: readBoolean(data.fallback) ?? undefined,
    terminal: readBoolean(data.terminal) ?? undefined,
  };
}

function parseWarningEvent(data: Record<string, unknown>): WarningEvent | null {
  const message = readString(data.message);
  const warning_type = readString(data.warning_type);
  if (message === null || warning_type !== 'persistence_degraded') return null;
  return {
    type: 'warning',
    warning_type,
    code: readString(data.code) ?? undefined,
    message,
  };
}

function parseRecoverTextChunkPayload(payload: Record<string, unknown>): RecoverTextChunkPayload | null {
  const content = readString(payload.content);
  return content !== null
    ? { content, reconstructed: readBoolean(payload.reconstructed) ?? undefined }
    : null;
}

function parseRecoverToolUsePayload(payload: Record<string, unknown>): RecoverToolUsePayload | null {
  const tool = readString(payload.tool);
  const tool_id = readString(payload.tool_id);
  if (!tool || !tool_id) return null;
  return { tool, tool_id };
}

function parseRecoverToolResultPayload(payload: Record<string, unknown>): RecoverToolResultPayload | null {
  const base = parseRecoverToolUsePayload(payload);
  const status = readString(payload.status);
  if (!base || !status) return null;
  return { ...base, status, preview: readString(payload.preview) ?? undefined };
}

function parseRecoverTurnCompletePayload(payload: Record<string, unknown>): RecoverTurnCompletePayload {
  return {
    token_count: readNumber(payload.token_count) ?? undefined,
    safety_status: readSafetyStatus(payload.safety_status) ?? undefined,
    reconstructed: readBoolean(payload.reconstructed) ?? undefined,
  };
}

class StudioChatStore {
  sessions = $state<StudioSession[]>([]);
  activeSessionId = $state<string | null>(null);
  loading = $state(false);
  sending = $state(false);
  error = $state('');
  /** WP5-E: Whether more sessions exist beyond current page. */
  hasMoreSessions = $state(false);
  /** Opaque cursor for loading the next Studio session page. */
  nextSessionsCursor = $state<string | null>(null);
  /** WP9-G: Persistence degradation warning from backend. */
  persistenceWarning = $state('');
  /** WP9-G: Provider error details for frontend display. */
  providerError = $state('');

  private unsubs: Array<() => void> = [];
  private resyncRegistered = false;

  // Streaming state.
  streaming = $state(false);
  streamingContent = $state('');
  streamingMessageId = $state<string | null>(null);
  activeExecutionId = $state<string | null>(null);
  activeToolCall = $state<{ tool: string; toolId: string } | null>(null);
  abortController: AbortController | null = null;
  private cancelledByUser = false;
  private lastStreamSeq = 0;
  private toolTimeoutId: ReturnType<typeof setTimeout> | null = null;
  /** WP9-M: Dedup guard — prevents duplicate events during stream/recovery race. */
  private processedEventIds = new Set<string>();
  private initialized = false;
  private sessionLoadPromises = new Map<string, Promise<void>>();

  get activeSession(): StudioSession | undefined {
    return this.sessions.find((s) => s.id === this.activeSessionId);
  }

  /** Load session list from API. */
  async init() {
    if (this.loading) return;
    if (this.initialized && this.sessions.length > 0) {
      this.error = '';
      return;
    }

    this.loading = true;
    this.error = '';
    try {
      const client = await getGhostClient();
      const data = await client.sessions.list({ limit: 50 });
      const loaded = (data.sessions ?? []).map((s: ApiStudioSession) => toStudioSession(s));
      this.sessions = loaded;
      this.hasMoreSessions = data.has_more ?? false;
      this.nextSessionsCursor = data.next_cursor ?? null;

      const savedId =
        typeof localStorage !== 'undefined'
          ? localStorage.getItem(STORAGE_KEY)
          : null;

      if (savedId && this.sessions.some((s) => s.id === savedId)) {
        await this.loadSession(savedId);
      } else if (this.sessions.length > 0) {
        await this.loadSession(this.sessions[0].id);
      }
      this.initialized = true;
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load sessions';
    }
    this.loading = false;

    // Register Resync handler on first init to reload sessions on reconnect gap.
    if (!this.resyncRegistered) {
      this.resyncRegistered = true;
      this.unsubs.push(
        wsStore.on('Resync', () => {
          // Don't re-init during active streaming — it would orphan the
          // session reference held by sendMessage's closure.
          if (this.streaming) {
            return;
          }
          // Stagger to avoid thundering herd on reconnect.
          // Use a lightweight reload of the session list rather than
          // full init() to avoid switching active session unexpectedly.
          setTimeout(async () => {
            try {
              const client = await getGhostClient();
              const data = await client.sessions.list({ limit: 50 });
              const refreshed = (data.sessions ?? []).map((s: ApiStudioSession) =>
                toStudioSession(s),
              );
              this.sessions = refreshed;
              this.hasMoreSessions = data.has_more ?? false;
              this.nextSessionsCursor = data.next_cursor ?? null;
              // Refresh the active session's messages if one is selected.
              if (this.activeSessionId) {
                await this.loadSession(this.activeSessionId);
              }
            } catch (e: unknown) {
              this.error = e instanceof Error ? e.message : 'Failed to refresh sessions';
            }
          }, Math.random() * 2000);
        }),
      );

      // Handle ChatMessage events — invalidate cached messages for
      // non-active sessions so next switchSession() triggers a reload.
      // If the active session completes elsewhere (for example after a page
      // reload interrupted SSE), refresh it immediately when we're not
      // already streaming locally.
      this.unsubs.push(
        wsStore.on('ChatMessage', (msg) => {
          const sessionId = msg.session_id as string;
          if (!sessionId) {
            return;
          }

          if (sessionId !== this.activeSessionId) {
            const session = this.sessions.find((s) => s.id === sessionId);
            if (session) {
              session.messages = [];
              this.sessions = [...this.sessions];
            }
            return;
          }

          if (!this.streaming) {
            void this.loadSession(sessionId);
          }
        }),
      );

      // Handle SessionEvent events — cross-tab/client awareness of
      // tool execution in other sessions.
      this.unsubs.push(
        wsStore.on('SessionEvent', (_msg) => {
          // Currently used for awareness only. Could display an activity
          // indicator on session list items showing active tool execution.
        }),
      );
    }
  }

  /** Create a new session and switch to it. */
  async createSession(opts?: {
    title?: string;
    model?: string;
    system_prompt?: string;
    temperature?: number;
    max_tokens?: number;
  }) {
    this.error = '';
    try {
      const client = await getGhostClient();
      const session = await client.sessions.create({
        title: opts?.title,
        model: opts?.model,
        system_prompt: opts?.system_prompt,
        temperature: opts?.temperature,
        max_tokens: opts?.max_tokens,
      });
      const newSession = toStudioSession(session);
      this.sessions = [newSession, ...this.sessions];
      this.setActiveSession(newSession.id);
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to create session';
    }
  }

  /** Load a session with its full message history. */
  async loadSession(id: string) {
    const inFlight = this.sessionLoadPromises.get(id);
    if (inFlight) {
      return inFlight;
    }

    const loadPromise = (async () => {
      this.error = '';
      try {
        const client = await getGhostClient();
        const data: StudioSessionWithMessages = await client.sessions.get(id);
        const loaded: StudioSession = {
          id: data.id,
          title: data.title,
          model: data.model,
          system_prompt: data.system_prompt,
          temperature: data.temperature,
          max_tokens: data.max_tokens,
          created_at: data.created_at,
          updated_at: data.updated_at,
          messages: data.messages ?? [],
        };

        const idx = this.sessions.findIndex((s) => s.id === id);
        if (idx >= 0) {
          this.sessions[idx] = loaded;
          this.sessions = [...this.sessions];
        } else {
          this.sessions = [loaded, ...this.sessions];
        }
        this.setActiveSession(id);
      } catch (e: unknown) {
        this.error = e instanceof Error ? e.message : 'Failed to load session';
      }
    })();

    this.sessionLoadPromises.set(id, loadPromise);
    try {
      await loadPromise;
    } finally {
      if (this.sessionLoadPromises.get(id) === loadPromise) {
        this.sessionLoadPromises.delete(id);
      }
    }
  }

  /** Delete a session. */
  async deleteSession(id: string) {
    this.error = '';
    try {
      const client = await getGhostClient();
      await client.sessions.delete(id);
      this.sessions = this.sessions.filter((s) => s.id !== id);

      if (this.activeSessionId === id) {
        if (this.sessions.length > 0) {
          await this.loadSession(this.sessions[0].id);
        } else {
          this.activeSessionId = null;
          this.persistActiveId(null);
        }
      }
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to delete session';
    }
  }

  /** WP5-E: Load next page of sessions (cursor-based). */
  async loadMoreSessions() {
    if (!this.hasMoreSessions || !this.nextSessionsCursor) return;
    this.error = '';
    try {
      const client = await getGhostClient();
      const data = await client.sessions.list({
        limit: 50,
        cursor: this.nextSessionsCursor,
      });
      const more = (data.sessions ?? []).map((s: ApiStudioSession) => toStudioSession(s));
      const existingIds = new Set(this.sessions.map((session) => session.id));
      this.sessions = [
        ...this.sessions,
        ...more.filter((session) => !existingIds.has(session.id)),
      ];
      this.hasMoreSessions = data.has_more ?? false;
      this.nextSessionsCursor = data.next_cursor ?? null;
    } catch (e: unknown) {
      this.error = e instanceof Error ? e.message : 'Failed to load more sessions';
    }
  }

  /** Refresh the active session after reconnect or window resume. */
  async refreshActiveSession() {
    if (!this.activeSessionId || this.streaming) {
      return;
    }

    await this.loadSession(this.activeSessionId);
  }

  /** Send a message with SSE streaming. */
  async sendMessage(content: string): Promise<StudioMessage | null> {
    const session = this.activeSession;
    if (!session) {
      this.error = 'No active session';
      return null;
    }

    this.sending = true;
    this.streaming = true;
    this.streamingContent = '';
    this.streamingMessageId = null;
    this.activeExecutionId = null;
    this.activeToolCall = null;
    this.error = '';

    // Optimistically add user message.
    const userMsg: StudioMessage = {
      id: crypto.randomUUID(),
      role: 'user',
      content,
      token_count: 0,
      safety_status: 'clean',
      created_at: new Date().toISOString(),
    };
    session.messages = [...session.messages, userMsg];

    // Add placeholder assistant message with a temporary ID so the Svelte
    // keyed {#each} doesn't change keys when stream_start arrives.
    const tempId = `_pending_${crypto.randomUUID()}`;
    const placeholderMsg: StudioMessage = {
      id: tempId,
      role: 'assistant',
      content: '',
      token_count: 0,
      safety_status: 'clean',
      created_at: new Date().toISOString(),
    };
    session.messages = [...session.messages, placeholderMsg];
    const assistantMsgIndex = session.messages.length - 1;

    this.abortController = new AbortController();
    this.cancelledByUser = false;
    this.lastStreamSeq = 0;
    // WP9-M: Clear dedup set on new stream start (not on recovery).
    this.processedEventIds = new Set<string>();
    let receivedStreamEnd = false;

    // WP9-L: Client heartbeat — POST every 30s so backend knows we're alive.
    const heartbeatInterval = setInterval(() => {
      void getGhostClient()
        .then((client) => client.runtimeSessions.heartbeat(session.id))
        .catch(() => {});
    }, 30_000);

    // Activity-based idle timeout: if no meaningful event for 120s, abort.
    // WP5-D: Triple effective timeout when a tool call is active (360s).
    let lastActivity = Date.now();
    const IDLE_TIMEOUT_MS = 120_000;
    const TOOL_IDLE_TIMEOUT_MS = 360_000;
    const activityMonitor = setInterval(() => {
      const timeout = this.activeToolCall ? TOOL_IDLE_TIMEOUT_MS : IDLE_TIMEOUT_MS;
      if (Date.now() - lastActivity > timeout) {
        clearInterval(activityMonitor);
        this.error = `Stream timed out — no activity for ${timeout / 1000} seconds`;
        this.abortController?.abort();
      }
    }, 5_000);

    try {
      const client = await getGhostClient();
      await client.chat.streamWithCallback(
        session.id,
        { content },
        (eventType, data, eventId) => {
          // Track the last received sequence ID for recovery.
          if (eventId) {
            const parsed = parseInt(eventId, 10);
            if (!isNaN(parsed) && parsed > this.lastStreamSeq) {
              this.lastStreamSeq = parsed;
            }

            // WP9-M: Dedup guard — skip already-processed events.
            if (this.processedEventIds.has(eventId)) return;
            this.processedEventIds.add(eventId);
            // Cap set at 10,000 entries to bound memory.
            if (this.processedEventIds.size > 10_000) {
              const arr = [...this.processedEventIds];
              this.processedEventIds = new Set(arr.slice(arr.length - 5_000));
            }
          }

          // Reset activity timer on any meaningful event.
          if (['text_delta', 'tool_use', 'tool_result', 'heartbeat', 'stream_start', 'stream_end', 'warning', 'error'].includes(eventType)) {
            lastActivity = Date.now();
          }

          switch (eventType) {
            case 'stream_start': {
              const event = parseStreamStartEvent(data);
              if (!event) break;
              this.activeExecutionId = event.execution_id ?? null;
              this.streamingMessageId = event.message_id;
              session.messages[assistantMsgIndex] = {
                ...session.messages[assistantMsgIndex],
                id: event.message_id,
              };
              session.messages = [...session.messages];
              break;
            }

            case 'text_delta': {
              const event = parseTextDeltaEvent(data);
              if (!event) break;
              this.providerError = '';
              if (event.reconstructed) {
                this.persistenceWarning = RECONSTRUCTED_REPLAY_WARNING;
              }
              this.streamingContent += event.content;
              session.messages[assistantMsgIndex] = {
                ...session.messages[assistantMsgIndex],
                content: this.streamingContent,
              };
              session.messages = [...session.messages];
              break;
            }

            case 'tool_use': {
              const event = parseToolUseEvent(data);
              if (!event) break;
              this.providerError = '';
              this.activeToolCall = { tool: event.tool, toolId: event.tool_id };
              const msg = session.messages[assistantMsgIndex];
              session.messages[assistantMsgIndex] = {
                ...msg,
                toolCalls: upsertToolCall(msg.toolCalls, {
                  tool: event.tool,
                  toolId: event.tool_id,
                  status: 'running',
                }),
              };
              session.messages = [...session.messages];

              // WP5-C: 5-minute tool call timeout.
              if (this.toolTimeoutId) clearTimeout(this.toolTimeoutId);
              this.toolTimeoutId = setTimeout(() => {
                const m = session.messages[assistantMsgIndex];
                const tc = (m.toolCalls ?? []).map((t) =>
                  t.toolId === event.tool_id && t.status === 'running'
                    ? { ...t, status: 'error' as const, preview: 'Tool may have timed out (5 min)' }
                    : t,
                );
                session.messages[assistantMsgIndex] = { ...m, toolCalls: tc };
                session.messages = [...session.messages];
                this.activeToolCall = null;
                this.toolTimeoutId = null;
              }, 300_000);
              break;
            }

            case 'tool_result': {
              const event = parseToolResultEvent(data);
              if (!event) break;
              this.providerError = '';
              this.activeToolCall = null;
              if (this.toolTimeoutId) { clearTimeout(this.toolTimeoutId); this.toolTimeoutId = null; }
              const msg2 = session.messages[assistantMsgIndex];
              session.messages[assistantMsgIndex] = {
                ...msg2,
                toolCalls: upsertToolCall(msg2.toolCalls, {
                  tool: event.tool,
                  toolId: event.tool_id,
                  status: event.status === 'error' ? 'error' : 'done',
                  preview: event.preview,
                }),
              };
              // Force reactivity flush after tool completion.
              session.messages = [...session.messages];
              break;
            }

            case 'stream_end': {
              const event = parseStreamEndEvent(data);
              if (!event) break;
              receivedStreamEnd = true;
              this.providerError = '';
              if (event.reconstructed) {
                this.persistenceWarning = RECONSTRUCTED_REPLAY_WARNING;
              }
              session.messages[assistantMsgIndex] = {
                ...session.messages[assistantMsgIndex],
                content: this.streamingContent,
                token_count: event.token_count,
                safety_status: event.safety_status,
                status: 'complete',
              };
              // Force array reassignment to ensure Svelte reactivity flush.
              session.messages = [...session.messages];
              break;
            }

            case 'error': {
              const event = parseErrorEvent(data);
              if (!event) break;

              if (event.terminal === false) {
                if (event.error_type === 'provider_unavailable') {
                  this.providerError = `Provider ${event.provider ?? 'unknown'} unavailable${event.fallback ? ' - trying fallback...' : ''}`;
                } else if (event.error_type === 'auth_failed') {
                  this.providerError = `Provider ${event.provider ?? 'provider'} authentication failed${event.fallback ? ' - trying fallback...' : ''}`;
                } else {
                  this.providerError = event.message;
                }
                break;
              }

              if (event.error_type === 'provider_unavailable') {
                this.providerError = `Provider ${event.provider ?? 'unknown'} unavailable`;
              } else if (event.error_type === 'auth_failed') {
                this.providerError = `Provider ${event.provider ?? 'provider'} authentication failed`;
              }

              this.error = event.message;
              session.messages[assistantMsgIndex] = {
                ...session.messages[assistantMsgIndex],
                content: this.streamingContent,
                status: 'error',
              };
              session.messages = [...session.messages];
              break;
            }

            case 'warning': {
              const event = parseWarningEvent(data);
              if (!event) break;
              if (event.warning_type === 'persistence_degraded') {
                this.persistenceWarning = 'Messages may not be saved - database contention detected';
              }
              break;
            }
          }
        },
        this.abortController.signal,
      );
    } catch (e: unknown) {
      if (!(e instanceof Error && e.name === 'AbortError')) {
        this.error = e instanceof Error ? e.message : 'Streaming failed';
        // Attempt stream recovery from persisted event log.
        const realMsgId = session.messages[assistantMsgIndex]?.id;
        if (realMsgId && !realMsgId.startsWith('_pending_')) {
          await this.recoverStream(session.id, realMsgId, session, assistantMsgIndex);
        }
      }
    } finally {
      clearInterval(activityMonitor);
      clearInterval(heartbeatInterval);

      // Detect incomplete stream: no stream_end received but we have content.
      if (
        !receivedStreamEnd &&
        !this.cancelledByUser &&
        this.streamingContent.length > 0 &&
        session.messages[assistantMsgIndex]?.status !== 'error'
      ) {
        // Try recovery from event log first.
        const realMsgId = session.messages[assistantMsgIndex]?.id;
        if (realMsgId && !realMsgId.startsWith('_pending_')) {
          await this.recoverStream(session.id, realMsgId, session, assistantMsgIndex);
        }

        // If still incomplete after recovery, mark as such and schedule reload.
        if (session.messages[assistantMsgIndex]?.status !== 'complete') {
          session.messages[assistantMsgIndex] = {
            ...session.messages[assistantMsgIndex],
            content: this.streamingContent,
            status: 'incomplete',
          };
          session.messages = [...session.messages];

          // Schedule a background reload in case the backend persisted
          // additional events before the stream ended unexpectedly.
          setTimeout(() => {
            if (!this.streaming && this.activeSessionId === session.id) {
              this.loadSession(session.id);
            }
          }, 5000);
        }
      }

      this.sending = false;
      this.streaming = false;
      this.streamingContent = '';
      this.streamingMessageId = null;
      this.activeExecutionId = null;
      this.activeToolCall = null;
      this.abortController = null;
      this.persistenceWarning = '';
      this.providerError = '';
      if (this.toolTimeoutId) { clearTimeout(this.toolTimeoutId); this.toolTimeoutId = null; }

      // Update session title for first message.
      if (session.title === 'New Chat' && content.trim()) {
        session.title = content.length > 60 ? content.slice(0, 57) + '...' : content;
      }

      // Move session to top.
      const idx = this.sessions.findIndex((s) => s.id === session.id);
      if (idx > 0) {
        this.sessions = [session, ...this.sessions.filter((s) => s.id !== session.id)];
      }
    }

    return session.messages[assistantMsgIndex] ?? null;
  }

  private async cancelRemoteExecution(executionId: string) {
    try {
      const client = await getGhostClient();
      await client.liveExecutions.cancel(executionId, { idempotency: 'required' });
    } catch (e: unknown) {
      this.error =
        e instanceof Error
          ? `Failed to cancel remote execution: ${e.message}`
          : 'Failed to cancel remote execution';
    }
  }

  /** Cancel an in-flight streaming response. */
  cancelStreaming() {
    const executionId = this.activeExecutionId;
    this.activeExecutionId = null;
    if (executionId) {
      void this.cancelRemoteExecution(executionId);
    }

    if (this.abortController) {
      this.cancelledByUser = true;
      this.abortController.abort();
      // Immediately reset UI state — don't wait for AbortError to propagate,
      // as WebKit (Tauri) may not interrupt reader.read() synchronously.
      this.sending = false;
      this.streaming = false;
      this.activeToolCall = null;

      // Preserve any partial content that was streamed before cancel.
      const session = this.activeSession;
      if (session && this.streamingContent) {
        const lastMsg = session.messages[session.messages.length - 1];
        if (lastMsg?.role === 'assistant') {
          session.messages[session.messages.length - 1] = {
            ...lastMsg,
            content: this.streamingContent + '\n\n*(cancelled)*',
          };
          session.messages = [...session.messages];
        }
      }
      this.streamingContent = '';
      this.streamingMessageId = null;
    }
  }

  /** Retry the last user message after an incomplete stream. */
  async retryLastMessage() {
    const session = this.activeSession;
    if (!session) return;

    // Find the last user message.
    const messages = session.messages;
    let lastUserIdx = -1;
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i].role === 'user') {
        lastUserIdx = i;
        break;
      }
    }
    if (lastUserIdx < 0) return;

    const lastUserContent = messages[lastUserIdx].content;

    // Remove everything from the last user message onward (the user message
    // itself + any incomplete/error assistant messages after it).
    // sendMessage() will re-add the user message and a new assistant placeholder.
    session.messages = [...messages.slice(0, lastUserIdx)];

    // Re-send.
    await this.sendMessage(lastUserContent);
  }

  /** Recover missed stream events after SSE disconnect.
   * Fetches persisted events from the server's event log and replays them. */
  private async recoverStream(
    sessionId: string,
    messageId: string,
    session: StudioSession,
    assistantMsgIndex: number,
  ) {
    try {
      const client = await getGhostClient();
      const data: RecoverStreamResult = await client.sessions.recoverStream(sessionId, {
        message_id: messageId,
        after_seq: this.lastStreamSeq,
      });

      if (!data.events?.length) return;

      // Replay recovered events in order.
      for (const event of data.events) {
        const payload = event.payload;
        if (event.reconstructed === true) {
          this.persistenceWarning = RECONSTRUCTED_REPLAY_WARNING;
        }

        switch (event.event_type) {
          case 'text_delta': {
            const parsed = parseRecoverTextChunkPayload(payload);
            if (parsed) {
              if (parsed.reconstructed) {
                this.persistenceWarning = RECONSTRUCTED_REPLAY_WARNING;
              }
              this.streamingContent += parsed.content;
            }
            break;
          }

          case 'tool_use': {
            const parsed = parseRecoverToolUsePayload(payload);
            if (!parsed) break;
            const msg = session.messages[assistantMsgIndex];
            session.messages[assistantMsgIndex] = {
              ...msg,
              toolCalls: upsertToolCall(msg.toolCalls, {
                tool: parsed.tool,
                toolId: parsed.tool_id,
                status: 'running',
              }),
            };
            break;
          }

          case 'tool_result': {
            const parsed = parseRecoverToolResultPayload(payload);
            if (!parsed) break;
            const msg2 = session.messages[assistantMsgIndex];
            session.messages[assistantMsgIndex] = {
              ...msg2,
              toolCalls: upsertToolCall(msg2.toolCalls, {
                tool: parsed.tool,
                toolId: parsed.tool_id,
                status: parsed.status === 'error' ? 'error' : 'done',
                preview: parsed.preview,
              }),
            };
            break;
          }

          case 'stream_end': {
            const parsed = parseRecoverTurnCompletePayload(payload);
            if (parsed.reconstructed) {
              this.persistenceWarning = RECONSTRUCTED_REPLAY_WARNING;
            }
            session.messages[assistantMsgIndex] = {
              ...session.messages[assistantMsgIndex],
              content: this.streamingContent,
              token_count: parsed.token_count ?? 0,
              safety_status: parsed.safety_status ?? 'clean',
              status: 'complete',
            };
            break;
          }

          case 'error':
            session.messages[assistantMsgIndex] = {
              ...session.messages[assistantMsgIndex],
              content: this.streamingContent,
              status: 'error',
            };
            break;
        }

        this.lastStreamSeq = event.seq;
      }

      // Update message content from accumulated text.
      if (session.messages[assistantMsgIndex]?.status !== 'complete') {
        session.messages[assistantMsgIndex] = {
          ...session.messages[assistantMsgIndex],
          content: this.streamingContent,
        };
      }

      // Force reactivity flush.
      session.messages = [...session.messages];
    } catch (e) {
      console.error('[studioChat] Stream recovery failed:', e);
      // Fallback: reload the full session from DB.
      try {
        await this.loadSession(sessionId);
      } catch { /* already handling errors */ }
    }
  }

  /** Switch to a different session (loads messages if needed). */
  async switchSession(id: string) {
    if (this.activeSessionId === id) return;
    const session = this.sessions.find((s) => s.id === id);
    if (session && session.messages.length > 0) {
      this.setActiveSession(id);
    } else {
      await this.loadSession(id);
    }
  }

  destroy() {
    for (const unsub of this.unsubs) unsub();
    this.unsubs = [];
    this.resyncRegistered = false;
  }

  private setActiveSession(id: string) {
    this.activeSessionId = id;
    this.persistActiveId(id);
  }

  private persistActiveId(id: string | null) {
    if (typeof localStorage !== 'undefined') {
      if (id) {
        localStorage.setItem(STORAGE_KEY, id);
      } else {
        localStorage.removeItem(STORAGE_KEY);
      }
    }
  }
}

export const studioChatStore = new StudioChatStore();
