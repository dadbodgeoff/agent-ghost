export type ThemeChoice = 'dark' | 'light' | 'system';

const STORAGE_KEY = 'ghost-theme';

function prefersLightTheme(): boolean {
  return typeof window !== 'undefined'
    && typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-color-scheme: light)').matches;
}

export function getStoredThemeChoice(): ThemeChoice {
  if (typeof localStorage === 'undefined') {
    return 'dark';
  }

  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'system') {
    return stored;
  }
  return 'dark';
}

export function applyThemeChoice(choice: ThemeChoice): void {
  if (typeof document === 'undefined') {
    return;
  }

  const root = document.documentElement;
  root.classList.remove('light');

  if (choice === 'light' || (choice === 'system' && prefersLightTheme())) {
    root.classList.add('light');
  }
}

export function persistThemeChoice(choice: ThemeChoice): void {
  if (typeof localStorage === 'undefined') {
    return;
  }
  localStorage.setItem(STORAGE_KEY, choice);
}

export function setThemeChoice(choice: ThemeChoice): void {
  persistThemeChoice(choice);
  applyThemeChoice(choice);
}

export function toggleThemeChoice(): ThemeChoice {
  const next = document.documentElement.classList.contains('light') ? 'dark' : 'light';
  setThemeChoice(next);
  return next;
}
