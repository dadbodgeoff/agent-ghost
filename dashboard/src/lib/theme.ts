import { readLocalStorage, writeLocalStorage } from '$lib/browser';

export type ThemeChoice = 'dark' | 'light' | 'system';

const THEME_STORAGE_KEY = 'ghost-theme';

export function readThemeChoice(): ThemeChoice {
  const stored = readLocalStorage(THEME_STORAGE_KEY);
  return stored === 'light' || stored === 'system' ? stored : 'dark';
}

export function applyThemeChoice(choice: ThemeChoice): void {
  if (typeof document === 'undefined') return;

  const html = document.documentElement;
  html.classList.remove('light');

  if (choice === 'light') {
    html.classList.add('light');
    return;
  }

  if (
    choice === 'system'
    && typeof window !== 'undefined'
    && typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-color-scheme: light)').matches
  ) {
    html.classList.add('light');
  }
}

export function setThemeChoice(choice: ThemeChoice): ThemeChoice {
  writeLocalStorage(THEME_STORAGE_KEY, choice);
  applyThemeChoice(choice);
  return choice;
}

export function toggleStoredThemeChoice(): ThemeChoice {
  const isLight = typeof document !== 'undefined' && document.documentElement.classList.contains('light');
  return setThemeChoice(isLight ? 'dark' : 'light');
}
