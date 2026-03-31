export type ThemeChoice = 'dark' | 'light' | 'system';

export const THEME_STORAGE_KEY = 'ghost-theme';

function prefersLightScheme(): boolean {
  return typeof window !== 'undefined'
    && typeof window.matchMedia === 'function'
    && window.matchMedia('(prefers-color-scheme: light)').matches;
}

export function readStoredTheme(): ThemeChoice {
  if (typeof localStorage === 'undefined') {
    return 'dark';
  }

  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return stored === 'light' || stored === 'system' ? stored : 'dark';
}

export function applyThemeChoice(choice: ThemeChoice): ThemeChoice {
  if (typeof document === 'undefined') {
    return choice;
  }

  const html = document.documentElement;
  html.classList.remove('light');

  if (choice === 'light' || (choice === 'system' && prefersLightScheme())) {
    html.classList.add('light');
  }

  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(THEME_STORAGE_KEY, choice);
  }

  return choice;
}

export function toggleStoredTheme(): ThemeChoice {
  const next = document.documentElement.classList.contains('light') ? 'dark' : 'light';
  return applyThemeChoice(next);
}
