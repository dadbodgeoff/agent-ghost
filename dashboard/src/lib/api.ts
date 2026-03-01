/**
 * REST API client for the GHOST gateway.
 *
 * WebSocket is now handled by the dedicated wsStore
 * (dashboard/src/lib/stores/websocket.svelte.ts).
 *
 * Token is read from sessionStorage (set by login page).
 */

const BASE_URL = 'http://127.0.0.1:18789';

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
  async get(path: string): Promise<any> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      headers: headers(),
      credentials: 'include',
    });
    if (resp.status === 401) {
      // Token expired or invalid — redirect to login.
      sessionStorage.removeItem('ghost-token');
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

  async post(path: string, body?: any): Promise<any> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'POST',
      headers: headers(),
      credentials: 'include',
      body: body ? JSON.stringify(body) : undefined,
    });
    if (resp.status === 401) {
      sessionStorage.removeItem('ghost-token');
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

  async put(path: string, body?: any): Promise<any> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'PUT',
      headers: headers(),
      credentials: 'include',
      body: body ? JSON.stringify(body) : undefined,
    });
    if (resp.status === 401) {
      sessionStorage.removeItem('ghost-token');
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

  async del(path: string): Promise<any> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'DELETE',
      headers: headers(),
      credentials: 'include',
    });
    if (resp.status === 401) {
      sessionStorage.removeItem('ghost-token');
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
