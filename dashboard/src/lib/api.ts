/**
 * Transitional REST helper.
 *
 * New code should prefer `$lib/ghost-client` and `@ghost/sdk`.
 * This file remains only to keep unmigrated routes working while transport
 * ownership moves to the runtime layer and SDK.
 */

import { clearToken } from '$lib/auth';
import { getRuntime } from '$lib/platform/runtime';

export async function getBaseUrl(): Promise<string> {
  const runtime = await getRuntime();
  return runtime.getBaseUrl();
}

async function buildUrl(path: string): Promise<string> {
  return `${await getBaseUrl()}${path}`;
}

async function headers(includeContentType = true): Promise<HeadersInit> {
  const runtime = await getRuntime();
  const token = await runtime.getToken();
  const h: HeadersInit = {
    Accept: 'application/json',
  };

  if (includeContentType) {
    h['Content-Type'] = 'application/json';
  }

  if (token) {
    h['Authorization'] = `Bearer ${token}`;
  }

  return h;
}

export const api = {
  async get<T = unknown>(path: string): Promise<T> {
    const resp = await fetch(await buildUrl(path), {
      headers: await headers(false),
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
      const body = await resp.json().catch(() => null);
      const msg = body?.error?.message || `HTTP ${resp.status}`;
      throw new Error(msg);
    }
    return resp.json();
  },

  async post<T = unknown>(path: string, body?: unknown): Promise<T | null> {
    const resp = await fetch(await buildUrl(path), {
      method: 'POST',
      headers: await headers(body !== undefined),
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

  async put<T = unknown>(path: string, body?: unknown): Promise<T | null> {
    const resp = await fetch(await buildUrl(path), {
      method: 'PUT',
      headers: await headers(body !== undefined),
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

  async streamPost(
    path: string,
    body: unknown,
    onEvent: (eventType: string, data: unknown, eventId?: string) => void,
    signal?: AbortSignal,
  ): Promise<void> {
    const resp = await fetch(await buildUrl(path), {
      method: 'POST',
      headers: await headers(true),
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
    const resp = await fetch(await buildUrl(path), {
      method: 'DELETE',
      headers: await headers(false),
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
