export type ThemeChoice = 'dark' | 'light' | 'system';

const THEME_STORAGE_KEY = 'ghost-theme';

function hasDocument(): boolean {
  return typeof document !== 'undefined';
}

function hasLocalStorage(): boolean {
  return typeof localStorage !== 'undefined';
}

export function readStoredTheme(): ThemeChoice {
  if (!hasLocalStorage()) {
    return 'dark';
  }

  const stored = localStorage.getItem(THEME_STORAGE_KEY);
  return stored === 'light' || stored === 'system' ? stored : 'dark';
}

export function applyThemeChoice(choice: ThemeChoice): void {
  if (!hasDocument()) {
    return;
  }

  const html = document.documentElement;
  html.classList.remove('light');

  if (choice === 'light') {
    html.classList.add('light');
    return;
  }

  if (
    choice === 'system'
    && typeof window !== 'undefined'
    && window.matchMedia('(prefers-color-scheme: light)').matches
  ) {
    html.classList.add('light');
  }
}

export function persistThemeChoice(choice: ThemeChoice): void {
  if (!hasLocalStorage()) {
    return;
  }
  localStorage.setItem(THEME_STORAGE_KEY, choice);
}

export function setThemeChoice(choice: ThemeChoice): ThemeChoice {
  persistThemeChoice(choice);
  applyThemeChoice(choice);
  return choice;
}

export function toggleStoredTheme(): ThemeChoice {
  if (!hasDocument()) {
    return readStoredTheme();
  }
  const next: ThemeChoice = document.documentElement.classList.contains('light') ? 'dark' : 'light';
  return setThemeChoice(next);
}
