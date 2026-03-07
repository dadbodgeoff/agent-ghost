/**
 * REST API client for the GHOST gateway.
 *
 * WebSocket is now handled by the dedicated wsStore
 * (dashboard/src/lib/stores/websocket.svelte.ts).
 *
 * Token is read from sessionStorage (set by login page).
 * 401 handling clears both Tauri store and sessionStorage via clearToken().
 */

import { clearToken } from '$lib/auth';

function getBaseUrl(): string {
  if (typeof localStorage !== 'undefined') {
    const override = localStorage.getItem('ghost-gateway-url');
    if (override) return override;
  }
  // Port injected by Tauri via window.eval at startup.
  if (typeof window !== 'undefined' && window.__GHOST_GATEWAY_PORT__) {
    return `http://127.0.0.1:${window.__GHOST_GATEWAY_PORT__}`;
  }
  // Vite env override for standalone dashboard dev.
  if (typeof import.meta !== 'undefined' && import.meta.env?.VITE_GHOST_GATEWAY_URL) {
    return import.meta.env.VITE_GHOST_GATEWAY_URL;
  }
  return 'http://127.0.0.1:39780';
}
const BASE_URL = getBaseUrl();
export { BASE_URL };

function getToken(): string | null {
  return sessionStorage.getItem('ghost-token');
}

function headers(): HeadersInit {
  const token = getToken();
  const h: HeadersInit = { 'Content-Type': 'application/json' };
  if (token) h['Authorization'] = `Bearer ${token}`;
  return h;
}

export const api = {
  async get<T = unknown>(path: string): Promise<T> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      headers: headers(),
      credentials: 'omit',
    });
    if (resp.status === 401) {
      // Token expired or invalid — redirect to login.
      await clearToken();
      if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
        window.location.href = '/login';
      }
      throw new Error('Unauthorized');
    }
    if (!resp.ok) {
      const body = await resp.json().catch(() => null);
      const msg = body?.error?.message || `HTTP ${resp.status}`;
      throw new Error(msg);
    }
    return resp.json();
  },

  async post<T = unknown>(path: string, body?: unknown): Promise<T | null> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'POST',
      headers: headers(),
      credentials: 'omit',
      body: body ? JSON.stringify(body) : undefined,
    });
    if (resp.status === 401) {
      await clearToken();
      if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
        window.location.href = '/login';
      }
      throw new Error('Unauthorized');
    }
    if (!resp.ok) {
      const errBody = await resp.json().catch(() => null);
      const msg = errBody?.error?.message || `HTTP ${resp.status}`;
      throw new Error(msg);
    }
    // Some endpoints (like DELETE) may return 204 No Content.
    const text = await resp.text();
    return text ? JSON.parse(text) : null;
  },

  async put<T = unknown>(path: string, body?: unknown): Promise<T | null> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'PUT',
      headers: headers(),
      credentials: 'omit',
      body: body ? JSON.stringify(body) : undefined,
    });
    if (resp.status === 401) {
      await clearToken();
      if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
        window.location.href = '/login';
      }
      throw new Error('Unauthorized');
    }
    if (!resp.ok) {
      const errBody = await resp.json().catch(() => null);
      const msg = errBody?.error?.message || `HTTP ${resp.status}`;
      throw new Error(msg);
    }
    const text = await resp.text();
    return text ? JSON.parse(text) : null;
  },

  /**
   * POST with SSE streaming response.
   * Calls onEvent for each SSE event, resolves when stream ends.
   */
  async streamPost(
    path: string,
    body: unknown,
    onEvent: (eventType: string, data: unknown, eventId?: string) => void,
    signal?: AbortSignal,
  ): Promise<void> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'POST',
      headers: headers(),
      credentials: 'omit',
      body: JSON.stringify(body),
      signal,
    });

    if (resp.status === 401) {
      await clearToken();
      if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
        window.location.href = '/login';
      }
      throw new Error('Unauthorized');
    }

    if (!resp.ok) {
      const errBody = await resp.json().catch(() => null);
      const msg = errBody?.error?.message || `HTTP ${resp.status}`;
      throw new Error(msg);
    }

    if (!resp.body) {
      throw new Error('Response body is null — streaming not supported');
    }

    const reader = resp.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let aborted = false;

    // Explicitly cancel the reader when abort fires. WebKit (Tauri)
    // doesn't always interrupt reader.read() from the fetch signal alone.
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

        // Parse SSE events from buffer.
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
            // Comments (: ping) are ignored.
          }

          if (dataLines.length > 0) {
            const dataStr = dataLines.join('\n');
            try {
              const data = JSON.parse(dataStr);
              onEvent(eventType, data, eventId);
            } catch {
              onEvent(eventType, dataStr, eventId);
            }
          }
        }
      }
    } finally {
      signal?.removeEventListener('abort', onAbort);
      reader.releaseLock();
    }
  },

  async del<T = unknown>(path: string): Promise<T | null> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'DELETE',
      headers: headers(),
      credentials: 'omit',
    });
    if (resp.status === 401) {
      await clearToken();
      if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
        window.location.href = '/login';
      }
      throw new Error('Unauthorized');
    }
    if (!resp.ok) {
      const errBody = await resp.json().catch(() => null);
      const msg = errBody?.error?.message || `HTTP ${resp.status}`;
      throw new Error(msg);
    }
    const text = await resp.text();
    return text ? JSON.parse(text) : null;
  },
};
