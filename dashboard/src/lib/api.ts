/**
 * REST + WebSocket API client (Task 6.9 AC2).
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
    const resp = await fetch(`${BASE_URL}${path}`, { headers: headers() });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    return resp.json();
  },

  async post(path: string, body?: any): Promise<any> {
    const resp = await fetch(`${BASE_URL}${path}`, {
      method: 'POST',
      headers: headers(),
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    return resp.json();
  },

  connectWebSocket(): WebSocket | null {
    const token = getToken();
    if (!token) return null;

    const ws = new WebSocket(`ws://127.0.0.1:18789/api/ws?token=${encodeURIComponent(token)}`);

    ws.onclose = () => {
      // Reconnect with exponential backoff
      setTimeout(() => api.connectWebSocket(), 3000);
    };

    return ws;
  },
};
