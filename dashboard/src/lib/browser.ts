export function readLocalStorage(key: string): string | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeLocalStorage(key: string, value: string): boolean {
  if (typeof localStorage === 'undefined') return false;
  try {
    localStorage.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

export function removeLocalStorage(key: string): boolean {
  if (typeof localStorage === 'undefined') return false;
  try {
    localStorage.removeItem(key);
    return true;
  } catch {
    return false;
  }
}

export function readSessionStorage(key: string): string | null {
  if (typeof sessionStorage === 'undefined') return null;
  try {
    return sessionStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeSessionStorage(key: string, value: string): boolean {
  if (typeof sessionStorage === 'undefined') return false;
  try {
    sessionStorage.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

export function removeSessionStorage(key: string): boolean {
  if (typeof sessionStorage === 'undefined') return false;
  try {
    sessionStorage.removeItem(key);
    return true;
  } catch {
    return false;
  }
}

export function generateUuid(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }

  return `ghost-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

export function isMacPlatform(): boolean {
  return typeof navigator !== 'undefined' && navigator.platform.includes('Mac');
}
