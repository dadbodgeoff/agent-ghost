import type { RuntimePlatform } from './runtime';

const TOKEN_KEY = 'ghost-token';
const CLIENT_ID_KEY = 'ghost-client-id';
const SESSION_EPOCH_KEY = 'ghost-session-epoch';
const listeners = new Set<(token: string | null) => void>();

function getLocalStorage(): Storage | null {
  return typeof localStorage === 'undefined' ? null : localStorage;
}

function getSessionStorage(): Storage | null {
  return typeof sessionStorage === 'undefined' ? null : sessionStorage;
}

function nextClientId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `ghost-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

function emitTokenChange(token: string | null) {
  for (const listener of listeners) {
    listener(token);
  }
}

function resolveBaseUrl(): string {
  const storage = getLocalStorage();
  if (storage) {
    const override = storage.getItem('ghost-gateway-url');
    if (override) {
      return override;
    }
  }

  if (typeof import.meta !== 'undefined' && import.meta.env?.VITE_GHOST_GATEWAY_URL) {
    return import.meta.env.VITE_GHOST_GATEWAY_URL;
  }

  return 'http://127.0.0.1:39780';
}

function resolveReplayClientId(): string {
  const storage = getLocalStorage();
  if (!storage) {
    return nextClientId();
  }

  const existing = storage.getItem(CLIENT_ID_KEY);
  if (existing) return existing;
  const clientId = nextClientId();
  storage.setItem(CLIENT_ID_KEY, clientId);
  return clientId;
}

function resolveReplaySessionEpoch(): number {
  const storage = getLocalStorage();
  if (!storage) {
    return 1;
  }

  const raw = storage.getItem(SESSION_EPOCH_KEY);
  const epoch = raw ? Number.parseInt(raw, 10) : 1;
  if (Number.isFinite(epoch) && epoch > 0) return epoch;
  storage.setItem(SESSION_EPOCH_KEY, '1');
  return 1;
}

export const webRuntime: RuntimePlatform = {
  kind: 'web',
  isDesktop: () => false,
  async getBaseUrl() {
    return resolveBaseUrl();
  },
  async getToken() {
    return getSessionStorage()?.getItem(TOKEN_KEY) ?? null;
  },
  async setToken(token: string) {
    getSessionStorage()?.setItem(TOKEN_KEY, token);
    emitTokenChange(token);
  },
  async clearToken() {
    getSessionStorage()?.removeItem(TOKEN_KEY);
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
    getLocalStorage()?.setItem(SESSION_EPOCH_KEY, String(next));
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
    if (typeof window !== 'undefined') {
      window.open(url, '_blank', 'noopener,noreferrer');
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
