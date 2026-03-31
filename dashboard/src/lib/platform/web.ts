import type { RuntimePlatform } from './runtime';

const TOKEN_KEY = 'ghost-token';
const CLIENT_ID_KEY = 'ghost-client-id';
const SESSION_EPOCH_KEY = 'ghost-session-epoch';
const listeners = new Set<(token: string | null) => void>();

function emitTokenChange(token: string | null) {
  for (const listener of listeners) {
    listener(token);
  }
}

function resolveBaseUrl(): string {
  if (typeof localStorage !== 'undefined') {
    const override = localStorage.getItem('ghost-gateway-url');
    if (override) return override;
  }

  if (typeof import.meta !== 'undefined' && import.meta.env?.VITE_GHOST_GATEWAY_URL) {
    return import.meta.env.VITE_GHOST_GATEWAY_URL;
  }

  return 'http://127.0.0.1:39780';
}

function resolveReplayClientId(): string {
  const existing = localStorage.getItem(CLIENT_ID_KEY);
  if (existing) return existing;
  const clientId = crypto.randomUUID();
  localStorage.setItem(CLIENT_ID_KEY, clientId);
  return clientId;
}

function resolveReplaySessionEpoch(): number {
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
    return sessionStorage.getItem(TOKEN_KEY);
  },
  async setToken(token: string) {
    sessionStorage.setItem(TOKEN_KEY, token);
    emitTokenChange(token);
  },
  async clearToken() {
    sessionStorage.removeItem(TOKEN_KEY);
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
    localStorage.setItem(SESSION_EPOCH_KEY, String(next));
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
  async subscribeWindowFocus() {
    return () => {};
  },
  async spawnTerminalPty() {
    return null;
  },
};
