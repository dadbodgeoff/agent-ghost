import type { RuntimePlatform } from './runtime';

const TOKEN_KEY = 'ghost-token';
const CLIENT_ID_KEY = 'ghost-client-id';
const SESSION_EPOCH_KEY = 'ghost-session-epoch';
const listeners = new Set<(token: string | null) => void>();

function hasLocalStorage(): boolean {
  return typeof localStorage !== 'undefined';
}

function hasSessionStorage(): boolean {
  return typeof sessionStorage !== 'undefined';
}

function readLocalStorage(key: string): string | null {
  if (!hasLocalStorage()) return null;
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeLocalStorage(key: string, value: string): void {
  if (!hasLocalStorage()) return;
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore storage write failures (for example, private browsing or quotas).
  }
}

function readSessionStorage(key: string): string | null {
  if (!hasSessionStorage()) return null;
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeSessionStorage(key: string, value: string): void {
  if (!hasSessionStorage()) return;
  try {
    sessionStorage.setItem(key, value);
  } catch {
    // Ignore storage write failures to avoid breaking auth flows.
  }
}

function removeSessionStorage(key: string): void {
  if (!hasSessionStorage()) return;
  try {
    sessionStorage.removeItem(key);
  } catch {
    // Ignore storage write failures to avoid breaking auth flows.
  }
}

function randomId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `ghost-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function emitTokenChange(token: string | null) {
  for (const listener of listeners) {
    listener(token);
  }
}

function resolveBaseUrl(): string {
  const override = readLocalStorage('ghost-gateway-url');
  if (override) return override;

  if (typeof import.meta !== 'undefined' && import.meta.env?.VITE_GHOST_GATEWAY_URL) {
    return import.meta.env.VITE_GHOST_GATEWAY_URL;
  }

  return 'http://127.0.0.1:39780';
}

function resolveReplayClientId(): string {
  const existing = readLocalStorage(CLIENT_ID_KEY);
  if (existing) return existing;
  const clientId = randomId();
  writeLocalStorage(CLIENT_ID_KEY, clientId);
  return clientId;
}

function resolveReplaySessionEpoch(): number {
  const raw = readLocalStorage(SESSION_EPOCH_KEY);
  const epoch = raw ? Number.parseInt(raw, 10) : 1;
  if (Number.isFinite(epoch) && epoch > 0) return epoch;
  writeLocalStorage(SESSION_EPOCH_KEY, '1');
  return 1;
}

export const webRuntime: RuntimePlatform = {
  kind: 'web',
  isDesktop: () => false,
  async getBaseUrl() {
    return resolveBaseUrl();
  },
  async getToken() {
    return readSessionStorage(TOKEN_KEY);
  },
  async setToken(token: string) {
    writeSessionStorage(TOKEN_KEY, token);
    emitTokenChange(token);
  },
  async clearToken() {
    removeSessionStorage(TOKEN_KEY);
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
    writeLocalStorage(SESSION_EPOCH_KEY, String(next));
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
    if (typeof window === 'undefined') return;
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
