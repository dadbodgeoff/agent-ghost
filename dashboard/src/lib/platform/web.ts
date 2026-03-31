import type { RuntimePlatform } from './runtime';

const TOKEN_KEY = 'ghost-token';
const CLIENT_ID_KEY = 'ghost-client-id';
const SESSION_EPOCH_KEY = 'ghost-session-epoch';
const listeners = new Set<(token: string | null) => void>();

function safeGetLocalStorage(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeSetLocalStorage(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage access can fail in privacy-restricted browsers.
  }
}

function safeGetSessionStorage(key: string): string | null {
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function safeSetSessionStorage(key: string, value: string): void {
  try {
    sessionStorage.setItem(key, value);
  } catch {
    // Ignore storage failures and continue with in-memory behavior.
  }
}

function safeRemoveSessionStorage(key: string): void {
  try {
    sessionStorage.removeItem(key);
  } catch {
    // Ignore storage failures and continue with in-memory behavior.
  }
}

function emitTokenChange(token: string | null) {
  for (const listener of listeners) {
    listener(token);
  }
}

function resolveBaseUrl(): string {
  if (typeof localStorage !== 'undefined') {
    const override = safeGetLocalStorage('ghost-gateway-url');
    if (override) return override;
  }

  if (typeof import.meta !== 'undefined' && import.meta.env?.VITE_GHOST_GATEWAY_URL) {
    return import.meta.env.VITE_GHOST_GATEWAY_URL;
  }

  return 'http://127.0.0.1:39780';
}

function resolveReplayClientId(): string {
  const existing = safeGetLocalStorage(CLIENT_ID_KEY);
  if (existing) return existing;
  const clientId =
    typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
      ? crypto.randomUUID()
      : `ghost-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  safeSetLocalStorage(CLIENT_ID_KEY, clientId);
  return clientId;
}

function resolveReplaySessionEpoch(): number {
  const raw = safeGetLocalStorage(SESSION_EPOCH_KEY);
  const epoch = raw ? Number.parseInt(raw, 10) : 1;
  if (Number.isFinite(epoch) && epoch > 0) return epoch;
  safeSetLocalStorage(SESSION_EPOCH_KEY, '1');
  return 1;
}

export const webRuntime: RuntimePlatform = {
  kind: 'web',
  isDesktop: () => false,
  async getBaseUrl() {
    return resolveBaseUrl();
  },
  async getToken() {
    return safeGetSessionStorage(TOKEN_KEY);
  },
  async setToken(token: string) {
    safeSetSessionStorage(TOKEN_KEY, token);
    emitTokenChange(token);
  },
  async clearToken() {
    safeRemoveSessionStorage(TOKEN_KEY);
    emitTokenChange(null);
  },
  async getReplayClientId() {
    return resolveReplayClientId();
  },
  async getReplaySessionEpoch() {
    return resolveReplaySessionEpoch();
  },
  async advanceReplaySessionEpoch() {
    const next = resolveReplaySessionEpoch() + 1;
    safeSetLocalStorage(SESSION_EPOCH_KEY, String(next));
    return next;
  },
  subscribeTokenChange(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
  async gatewayStatus() {
    return 'unknown';
  },
  async startGateway() {
    throw new Error('Gateway lifecycle control is only available in the desktop app');
  },
  async stopGateway() {
    throw new Error('Gateway lifecycle control is only available in the desktop app');
  },
  async openExternalUrl(url: string) {
    const opened = window.open(url, '_blank', 'noopener,noreferrer');
    if (!opened) {
      window.location.assign(url);
    }
  },
  async requestNotificationPermission() {
    if (typeof Notification === 'undefined') {
      return false;
    }
    if (Notification.permission === 'granted') {
      return true;
    }
    return (await Notification.requestPermission()) === 'granted';
  },
  async sendNotification(notification) {
    if (typeof Notification === 'undefined' || Notification.permission !== 'granted') {
      return;
    }
    new Notification(notification.title, notification.body ? { body: notification.body } : undefined);
  },
  async readKeybindings() {
    return [];
  },
  async spawnTerminalPty() {
    return null;
  },
};
