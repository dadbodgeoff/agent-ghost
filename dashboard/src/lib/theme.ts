import { readLocalStorage, writeLocalStorage } from '$lib/browser-storage';

export type ThemeChoice = 'dark' | 'light' | 'system';

export const THEME_STORAGE_KEY = 'ghost-theme';

export function readThemeChoice(): ThemeChoice {
  const stored = readLocalStorage(THEME_STORAGE_KEY);
  if (stored === 'light' || stored === 'system') {
    return stored;
  }
  return 'dark';
}

export function applyThemeChoice(choice: ThemeChoice): void {
  if (typeof document === 'undefined') return;

  const root = document.documentElement;
  root.classList.remove('light');

  if (choice === 'light') {
    root.classList.add('light');
    return;
  }

  if (
    choice === 'system'
    && typeof window !== 'undefined'
    && window.matchMedia('(prefers-color-scheme: light)').matches
  ) {
    root.classList.add('light');
  }
}

export function persistThemeChoice(choice: ThemeChoice): void {
  writeLocalStorage(THEME_STORAGE_KEY, choice);
}

export function toggleThemeChoice(): ThemeChoice {
  const current = readThemeChoice();
  const next: ThemeChoice = current === 'light' ? 'dark' : 'light';
  persistThemeChoice(next);
  applyThemeChoice(next);
  return next;
}
