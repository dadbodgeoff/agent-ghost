export type ThemeChoice = 'dark' | 'light' | 'system';

function storageAvailable(kind: 'localStorage' | 'sessionStorage'): boolean {
  return typeof window !== 'undefined' && kind in window;
}

export function readLocalStorage(key: string): string | null {
  if (!storageAvailable('localStorage')) return null;
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeLocalStorage(key: string, value: string): void {
  if (!storageAvailable('localStorage')) return;
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Ignore storage quota/private mode failures.
  }
}

export function removeLocalStorage(key: string): void {
  if (!storageAvailable('localStorage')) return;
  try {
    window.localStorage.removeItem(key);
  } catch {
    // Ignore storage quota/private mode failures.
  }
}

export function readSessionStorage(key: string): string | null {
  if (!storageAvailable('sessionStorage')) return null;
  try {
    return window.sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeSessionStorage(key: string, value: string): void {
  if (!storageAvailable('sessionStorage')) return;
  try {
    window.sessionStorage.setItem(key, value);
  } catch {
    // Ignore storage quota/private mode failures.
  }
}

export function removeSessionStorage(key: string): void {
  if (!storageAvailable('sessionStorage')) return;
  try {
    window.sessionStorage.removeItem(key);
  } catch {
    // Ignore storage quota/private mode failures.
  }
}

export function prefersLightTheme(): boolean {
  return typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: light)').matches;
}

export function getStoredThemeChoice(): ThemeChoice {
  const stored = readLocalStorage('ghost-theme');
  if (stored === 'light' || stored === 'system') return stored;
  return 'dark';
}

export function applyThemeChoice(choice: ThemeChoice): ThemeChoice {
  if (typeof document === 'undefined') return choice;
  const html = document.documentElement;
  html.classList.remove('light');
  if (choice === 'light' || (choice === 'system' && prefersLightTheme())) {
    html.classList.add('light');
  }
  writeLocalStorage('ghost-theme', choice);
  return choice;
}

export function applyStoredThemeChoice(): ThemeChoice {
  return applyThemeChoice(getStoredThemeChoice());
}

export function toggleStoredTheme(): ThemeChoice {
  const next = document.documentElement.classList.contains('light') ? 'dark' : 'light';
  return applyThemeChoice(next);
}

export function supportsServiceWorker(): boolean {
  return typeof navigator !== 'undefined' && 'serviceWorker' in navigator;
}

export function supportsPushNotifications(): boolean {
  return typeof window !== 'undefined' && 'PushManager' in window && typeof Notification !== 'undefined';
}

export function hasClipboardWrite(): boolean {
  return typeof navigator !== 'undefined' && !!navigator.clipboard?.writeText;
}

export function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `ghost-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}
