/**
 * Auth utilities — dual-mode token storage.
 *
 * In Tauri: persists to tauri-plugin-store (survives app restart),
 *           AND writes to sessionStorage (api.ts reads it synchronously).
 * In browser: sessionStorage only (cleared on tab close).
 *
 * Design note: api.ts reads sessionStorage synchronously in its headers()
 * function. We do NOT make api.ts async. Instead, setToken() always writes
 * to both stores, and +layout.svelte hydrates sessionStorage from the
 * Tauri store on mount before any API calls.
 */

const TOKEN_KEY = 'ghost-token';
const isTauri = typeof window !== 'undefined' && !!window.__TAURI__;

let tauriStore: any = null;
async function getStore() {
  if (!isTauri) return null;
  if (!tauriStore) {
    const { LazyStore } = await import('@tauri-apps/plugin-store');
    tauriStore = new LazyStore('auth.json');
  }
  return tauriStore;
}

export async function getToken(): Promise<string | null> {
  const store = await getStore();
  if (store) {
    return (await store.get(TOKEN_KEY)) as string | null;
  }
  return sessionStorage.getItem(TOKEN_KEY);
}

export async function setToken(token: string): Promise<void> {
  const store = await getStore();
  if (store) {
    await store.set(TOKEN_KEY, token);
    await store.save();
  }
  // Always write to sessionStorage — api.ts reads it synchronously
  sessionStorage.setItem(TOKEN_KEY, token);
}

export async function clearToken(): Promise<void> {
  const store = await getStore();
  if (store) {
    await store.delete(TOKEN_KEY);
    await store.save();
  }
  sessionStorage.removeItem(TOKEN_KEY);
}

export function isAuthenticated(): boolean {
  // Sync check — sessionStorage is always populated by setToken + layout hydration
  return sessionStorage.getItem(TOKEN_KEY) !== null;
}
