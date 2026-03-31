import type { RuntimePlatform } from './runtime';

const TOKEN_KEY = 'ghost-token';
const CLIENT_ID_KEY = 'ghost-client-id';
const SESSION_EPOCH_KEY = 'ghost-session-epoch';
const listeners = new Set<(token: string | null) => void>();

function storageAvailable(storage: 'localStorage' | 'sessionStorage'): boolean {
  return typeof window !== 'undefined' && storage in window && window[storage] != null;
}

function normalizeBaseUrl(url: string): string {
  const trimmed = url.trim();
  return trimmed ? trimmed.replace(/\/+$/, '') : 'http://127.0.0.1:39780';
}

function emitTokenChange(token: string | null) {
  for (const listener of listeners) {
    listener(token);
  }
}

function resolveBaseUrl(): string {
  if (storageAvailable('localStorage')) {
    const override = localStorage.getItem('ghost-gateway-url');
    if (override) return normalizeBaseUrl(override);
  }

  if (typeof import.meta !== 'undefined' && import.meta.env?.VITE_GHOST_GATEWAY_URL) {
    return normalizeBaseUrl(import.meta.env.VITE_GHOST_GATEWAY_URL);
  }

  return 'http://127.0.0.1:39780';
}

function resolveReplayClientId(): string {
  if (!storageAvailable('localStorage')) {
    return crypto.randomUUID();
  }
  const existing = localStorage.getItem(CLIENT_ID_KEY);
  if (existing) return existing;
  const clientId = crypto.randomUUID();
  localStorage.setItem(CLIENT_ID_KEY, clientId);
  return clientId;
}

function resolveReplaySessionEpoch(): number {
  if (!storageAvailable('localStorage')) {
    return 1;
  }
  const raw = localStorage.getItem(SESSION_EPOCH_KEY);
  const epoch = raw ? Number.parseInt(raw, 10) : 1;
  if (Number.isFinite(epoch) && epoch > 0) return epoch;
  localStorage.setItem(SESSION_EPOCH_KEY, '1');
  return 1;
}

export const webRuntime: RuntimePlatform = {
  kind: 'web',
  isDesktop: () => false,
  async getBaseUrl() {
    return resolveBaseUrl();
  },
  async getToken() {
    return storageAvailable('sessionStorage') ? sessionStorage.getItem(TOKEN_KEY) : null;
  },
  async setToken(token: string) {
    if (storageAvailable('sessionStorage')) {
      sessionStorage.setItem(TOKEN_KEY, token);
    }
    emitTokenChange(token);
  },
  async clearToken() {
    if (storageAvailable('sessionStorage')) {
      sessionStorage.removeItem(TOKEN_KEY);
    }
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
    if (storageAvailable('localStorage')) {
      localStorage.setItem(SESSION_EPOCH_KEY, String(next));
    }
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
    window.open(url, '_blank', 'noopener,noreferrer');
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
