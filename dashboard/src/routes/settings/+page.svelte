<script lang="ts">
  import { clearToken } from '$lib/auth';
  import { goto } from '$app/navigation';

  type ThemeChoice = 'dark' | 'light' | 'system';

  let theme: ThemeChoice = $state('dark');

  // Initialize from localStorage on mount.
  $effect(() => {
    const stored = localStorage.getItem('ghost-theme');
    if (stored === 'light' || stored === 'system') {
      theme = stored;
    } else {
      theme = 'dark';
    }
  });

  function setTheme(choice: ThemeChoice) {
    theme = choice;
    localStorage.setItem('ghost-theme', choice);

    const html = document.documentElement;
    html.classList.remove('light');

    if (choice === 'light') {
      html.classList.add('light');
    } else if (choice === 'system') {
      if (window.matchMedia('(prefers-color-scheme: light)').matches) {
        html.classList.add('light');
      }
    }
    // 'dark' = no .light class = dark theme (default).
  }

  async function logout() {
    await clearToken();
    goto('/login');
  }
</script>

<h1 class="page-title">Settings</h1>

<div class="section">
  <h2 class="section-title">Theme</h2>
  <p class="section-desc">Choose your preferred color scheme.</p>
  <div class="theme-options" role="radiogroup" aria-label="Theme selection">
    <button
      class="theme-btn"
      class:active={theme === 'dark'}
      onclick={() => setTheme('dark')}
      role="radio"
      aria-checked={theme === 'dark'}
    >
      <span class="theme-icon" aria-hidden="true">🌙</span>
      Dark
    </button>
    <button
      class="theme-btn"
      class:active={theme === 'light'}
      onclick={() => setTheme('light')}
      role="radio"
      aria-checked={theme === 'light'}
    >
      <span class="theme-icon" aria-hidden="true">☀️</span>
      Light
    </button>
    <button
      class="theme-btn"
      class:active={theme === 'system'}
      onclick={() => setTheme('system')}
      role="radio"
      aria-checked={theme === 'system'}
    >
      <span class="theme-icon" aria-hidden="true">💻</span>
      System
    </button>
  </div>
</div>

<div class="section">
  <h2 class="section-title">Authentication</h2>
  <p class="section-desc">Sign out of the current session.</p>
  <button class="logout-btn" onclick={logout}>Logout</button>
</div>

<div class="section">
  <h2 class="section-title">Convergence Profile</h2>
  <p class="section-desc">Current profile: <code>standard</code></p>
  <p class="section-hint">Profile editing will be available in Phase 3.</p>
</div>

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  .section {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--layout-card-padding);
    margin-bottom: var(--spacing-4);
  }

  .section-title {
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    margin-bottom: var(--spacing-1);
  }

  .section-desc {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-4);
  }

  .section-hint {
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
    margin-top: var(--spacing-2);
  }

  .section-hint code,
  .section-desc code {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    color: var(--color-brand-primary);
  }

  .theme-options {
    display: flex;
    gap: var(--spacing-2);
  }

  .theme-btn {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-bg-elevated-3);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    transition: border-color var(--duration-fast) var(--easing-default),
                background var(--duration-fast) var(--easing-default);
  }

  .theme-btn:hover {
    background: var(--color-surface-hover);
  }

  .theme-btn.active {
    border-color: var(--color-brand-primary);
    color: var(--color-brand-primary);
    background: var(--color-brand-subtle);
  }

  .theme-icon {
    font-size: var(--font-size-md);
  }

  .logout-btn {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-bg-elevated-3);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    transition: background var(--duration-fast) var(--easing-default);
  }

  .logout-btn:hover {
    background: var(--color-severity-hard-bg);
    border-color: var(--color-severity-hard);
    color: var(--color-severity-hard);
  }
</style>
