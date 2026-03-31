import type { RuntimePlatform } from './runtime';

const TOKEN_KEY = 'ghost-token';
const CLIENT_ID_KEY = 'ghost-client-id';
const SESSION_EPOCH_KEY = 'ghost-session-epoch';
const listeners = new Set<(token: string | null) => void>();

function canUseLocalStorage(): boolean {
  return typeof localStorage !== 'undefined';
}

function canUseSessionStorage(): boolean {
  return typeof sessionStorage !== 'undefined';
}

function getLocalStorageItem(key: string): string | null {
  if (!canUseLocalStorage()) return null;
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function setLocalStorageItem(key: string, value: string): void {
  if (!canUseLocalStorage()) return;
  try {
    localStorage.setItem(key, value);
  } catch {
    // Ignore storage failures in constrained browser contexts.
  }
}

function getSessionStorageItem(key: string): string | null {
  if (!canUseSessionStorage()) return null;
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

function setSessionStorageItem(key: string, value: string): void {
  if (!canUseSessionStorage()) return;
  try {
    sessionStorage.setItem(key, value);
  } catch {
    // Ignore storage failures in constrained browser contexts.
  }
}

function removeSessionStorageItem(key: string): void {
  if (!canUseSessionStorage()) return;
  try {
    sessionStorage.removeItem(key);
  } catch {
    // Ignore storage failures in constrained browser contexts.
  }
}

function canUseWindow(): boolean {
  return typeof window !== 'undefined';
}

function createRandomId(): string {
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
  const override = getLocalStorageItem('ghost-gateway-url');
  if (override) return override;

  if (typeof import.meta !== 'undefined' && import.meta.env?.VITE_GHOST_GATEWAY_URL) {
    return import.meta.env.VITE_GHOST_GATEWAY_URL;
  }

  return 'http://127.0.0.1:39780';
}

function resolveReplayClientId(): string {
  const existing = getLocalStorageItem(CLIENT_ID_KEY);
  if (existing) return existing;
  const clientId = createRandomId();
  setLocalStorageItem(CLIENT_ID_KEY, clientId);
  return clientId;
}

function resolveReplaySessionEpoch(): number {
  const raw = getLocalStorageItem(SESSION_EPOCH_KEY);
  const epoch = raw ? Number.parseInt(raw, 10) : 1;
  if (Number.isFinite(epoch) && epoch > 0) return epoch;
  setLocalStorageItem(SESSION_EPOCH_KEY, '1');
  return 1;
}

export const webRuntime: RuntimePlatform = {
  kind: 'web',
  isDesktop: () => false,
  async getBaseUrl() {
    return resolveBaseUrl();
  },
  async getToken() {
    return getSessionStorageItem(TOKEN_KEY);
  },
  async setToken(token: string) {
    setSessionStorageItem(TOKEN_KEY, token);
    emitTokenChange(token);
  },
  async clearToken() {
    removeSessionStorageItem(TOKEN_KEY);
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
    setLocalStorageItem(SESSION_EPOCH_KEY, String(next));
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
    if (!canUseWindow()) return;
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
