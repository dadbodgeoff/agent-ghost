/**
 * Studio chat session store — Svelte 5 runes.
 *
 * DB-backed session persistence via REST API.
 * Supports SSE streaming for real-time token display.
 * Persists activeSessionId to localStorage for cross-navigation restore.
 */

import { api } from '$lib/api';

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
  safety_status: string;
  created_at: string;
  toolCalls?: ToolCallEntry[];
}

const STORAGE_KEY = 'ghost-studio-active-session';

class StudioChatStore {
  sessions = $state<StudioSession[]>([]);
  activeSessionId = $state<string | null>(null);
  loading = $state(false);
  sending = $state(false);
  error = $state('');

  // Streaming state.
  streaming = $state(false);
  streamingContent = $state('');
  streamingMessageId = $state<string | null>(null);
  activeToolCall = $state<{ tool: string; toolId: string } | null>(null);
  abortController: AbortController | null = null;

  get activeSession(): StudioSession | undefined {
    return this.sessions.find((s) => s.id === this.activeSessionId);
  }

  /** Load session list from API. */
  async init() {
    this.loading = true;
    this.error = '';
    try {
      const data = await api.get('/api/studio/sessions?limit=100');
      this.sessions = (data.sessions ?? []).map((s: any) => ({
        ...s,
        messages: s.messages ?? [],
      }));

      const savedId =
        typeof localStorage !== 'undefined'
          ? localStorage.getItem(STORAGE_KEY)
          : null;

      if (savedId && this.sessions.some((s) => s.id === savedId)) {
        await this.loadSession(savedId);
      } else if (this.sessions.length > 0) {
        await this.loadSession(this.sessions[0].id);
      }
    } catch (e: any) {
      this.error = e.message || 'Failed to load sessions';
    }
    this.loading = false;
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
      const session = await api.post('/api/studio/sessions', {
        title: opts?.title,
        model: opts?.model,
        system_prompt: opts?.system_prompt,
        temperature: opts?.temperature,
        max_tokens: opts?.max_tokens,
      });
      const newSession: StudioSession = { ...session, messages: [] };
      this.sessions = [newSession, ...this.sessions];
      this.setActiveSession(newSession.id);
    } catch (e: any) {
      this.error = e.message || 'Failed to create session';
    }
  }

  /** Load a session with its full message history. */
  async loadSession(id: string) {
    this.error = '';
    try {
      const data = await api.get(`/api/studio/sessions/${id}`);
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
      } else {
        this.sessions = [loaded, ...this.sessions];
      }
      this.setActiveSession(id);
    } catch (e: any) {
      this.error = e.message || 'Failed to load session';
    }
  }

  /** Delete a session. */
  async deleteSession(id: string) {
    this.error = '';
    try {
      await api.del(`/api/studio/sessions/${id}`);
      this.sessions = this.sessions.filter((s) => s.id !== id);

      if (this.activeSessionId === id) {
        if (this.sessions.length > 0) {
          await this.loadSession(this.sessions[0].id);
        } else {
          this.activeSessionId = null;
          this.persistActiveId(null);
        }
      }
    } catch (e: any) {
      this.error = e.message || 'Failed to delete session';
    }
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

    // Add placeholder assistant message.
    const placeholderMsg: StudioMessage = {
      id: '',
      role: 'assistant',
      content: '',
      token_count: 0,
      safety_status: 'clean',
      created_at: new Date().toISOString(),
    };
    session.messages = [...session.messages, placeholderMsg];
    const assistantMsgIndex = session.messages.length - 1;

    this.abortController = new AbortController();

    try {
      await api.streamPost(
        `/api/studio/sessions/${session.id}/messages/stream`,
        { content },
        (eventType, data) => {
          switch (eventType) {
            case 'stream_start':
              this.streamingMessageId = data.message_id;
              session.messages[assistantMsgIndex] = {
                ...session.messages[assistantMsgIndex],
                id: data.message_id,
              };
              break;

            case 'text_delta':
              this.streamingContent += data.content;
              session.messages[assistantMsgIndex] = {
                ...session.messages[assistantMsgIndex],
                content: this.streamingContent,
              };
              break;

            case 'tool_use': {
              this.activeToolCall = { tool: data.tool, toolId: data.tool_id };
              const msg = session.messages[assistantMsgIndex];
              const calls = msg.toolCalls ?? [];
              calls.push({ tool: data.tool, toolId: data.tool_id, status: 'running' });
              session.messages[assistantMsgIndex] = { ...msg, toolCalls: [...calls] };
              break;
            }

            case 'tool_result': {
              this.activeToolCall = null;
              const msg2 = session.messages[assistantMsgIndex];
              const calls2 = (msg2.toolCalls ?? []).map((tc) =>
                tc.toolId === data.tool_id
                  ? { ...tc, status: data.status === 'error' ? 'error' as const : 'done' as const, preview: data.preview }
                  : tc,
              );
              session.messages[assistantMsgIndex] = { ...msg2, toolCalls: calls2 };
              break;
            }

            case 'stream_end':
              session.messages[assistantMsgIndex] = {
                ...session.messages[assistantMsgIndex],
                content: this.streamingContent,
                token_count: data.token_count ?? 0,
                safety_status: data.safety_status ?? 'clean',
              };
              break;

            case 'error':
              this.error = data.message;
              break;
          }
        },
        this.abortController.signal,
      );
    } catch (e: any) {
      if (e.name !== 'AbortError') {
        this.error = e.message || 'Streaming failed';
      }
    } finally {
      this.sending = false;
      this.streaming = false;
      this.streamingContent = '';
      this.streamingMessageId = null;
      this.activeToolCall = null;
      this.abortController = null;

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

  /** Cancel an in-flight streaming response. */
  cancelStreaming() {
    if (this.abortController) {
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
        }
      }
      this.streamingContent = '';
      this.streamingMessageId = null;
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
