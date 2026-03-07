import { invoke } from '@tauri-apps/api/core';
import type { RuntimePlatform } from './runtime';

const TOKEN_KEY = 'ghost-token';
const listeners = new Set<(token: string | null) => void>();

let storePromise: Promise<{
  get(key: string): Promise<unknown>;
  set(key: string, value: unknown): Promise<void>;
  delete(key: string): Promise<boolean>;
  save(): Promise<void>;
}> | null = null;

async function getStore() {
  if (!storePromise) {
    storePromise = import('@tauri-apps/plugin-store').then(({ LazyStore }) => new LazyStore('auth.json'));
  }

  return storePromise;
}

function emitTokenChange(token: string | null) {
  for (const listener of listeners) {
    listener(token);
  }
}

async function resolvePort(): Promise<number> {
  try {
    return await invoke<number>('gateway_port');
  } catch {
    return 39780;
  }
}

export const tauriRuntime: RuntimePlatform = {
  kind: 'tauri',
  isDesktop: () => true,
  async getBaseUrl() {
    return `http://127.0.0.1:${await resolvePort()}`;
  },
  async getToken() {
    const store = await getStore();
    return (await store.get(TOKEN_KEY)) as string | null;
  },
  async setToken(token: string) {
    const store = await getStore();
    await store.set(TOKEN_KEY, token);
    await store.save();
    emitTokenChange(token);
  },
  async clearToken() {
    const store = await getStore();
    await store.delete(TOKEN_KEY);
    await store.save();
    emitTokenChange(null);
  },
  subscribeTokenChange(listener) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
  async gatewayStatus() {
    return invoke<string>('gateway_status');
  },
  async startGateway() {
    return invoke<string>('start_gateway');
  },
  async stopGateway() {
    return invoke<string>('stop_gateway');
  },
};
