const THEME_KEY = 'ghost-theme';

export type ThemeChoice = 'dark' | 'light' | 'system';

export function readStoredTheme(): ThemeChoice {
  const stored = localStorage.getItem(THEME_KEY);
  if (stored === 'light' || stored === 'system') {
    return stored;
  }
  return 'dark';
}

export function applyThemeChoice(choice: ThemeChoice): void {
  localStorage.setItem(THEME_KEY, choice);

  const html = document.documentElement;
  html.classList.remove('light');

  if (choice === 'light') {
    html.classList.add('light');
    return;
  }

  if (choice === 'system' && window.matchMedia('(prefers-color-scheme: light)').matches) {
    html.classList.add('light');
  }
}
