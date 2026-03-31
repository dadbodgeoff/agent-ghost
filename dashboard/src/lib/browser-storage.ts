function safeStorageGet(storage: Storage | undefined, key: string): string | null {
  if (!storage) return null;
  try {
    return storage.getItem(key);
  } catch {
    return null;
  }
}

function safeStorageSet(storage: Storage | undefined, key: string, value: string): void {
  if (!storage) return;
  try {
    storage.setItem(key, value);
  } catch {
    // Ignore persistence failures so UI state can still proceed in-memory.
  }
}

function safeStorageRemove(storage: Storage | undefined, key: string): void {
  if (!storage) return;
  try {
    storage.removeItem(key);
  } catch {
    // Ignore persistence failures so logout/local resets do not hard-fail.
  }
}

export function readLocalStorage(key: string): string | null {
  return typeof localStorage === 'undefined' ? null : safeStorageGet(localStorage, key);
}

export function writeLocalStorage(key: string, value: string): void {
  if (typeof localStorage === 'undefined') return;
  safeStorageSet(localStorage, key, value);
}

export function removeLocalStorage(key: string): void {
  if (typeof localStorage === 'undefined') return;
  safeStorageRemove(localStorage, key);
}

export function readSessionStorage(key: string): string | null {
  return typeof sessionStorage === 'undefined' ? null : safeStorageGet(sessionStorage, key);
}

export function writeSessionStorage(key: string, value: string): void {
  if (typeof sessionStorage === 'undefined') return;
  safeStorageSet(sessionStorage, key, value);
}

export function removeSessionStorage(key: string): void {
  if (typeof sessionStorage === 'undefined') return;
  safeStorageRemove(sessionStorage, key);
}
