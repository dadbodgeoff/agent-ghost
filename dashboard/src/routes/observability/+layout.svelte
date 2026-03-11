<script lang="ts">
  import { page } from '$app/state';

  let { children } = $props();

  const tabs = [
    { href: '/observability/traces', label: 'Traces' },
    { href: '/observability/ade', label: 'ADE Health' },
  ];
</script>

<div class="observability-shell">
  <header class="observability-header">
    <div>
      <h1>Observability</h1>
      <p>Runtime traces and ADE control-plane health.</p>
    </div>
    <nav class="observability-nav" aria-label="Observability sections">
      {#each tabs as tab}
        <a
          href={tab.href}
          class:active={page.url.pathname === tab.href}
          aria-current={page.url.pathname === tab.href ? 'page' : undefined}
        >
          {tab.label}
        </a>
      {/each}
    </nav>
  </header>

  {@render children?.()}
</div>

<style>
  .observability-shell {
    padding: var(--spacing-6);
  }

  .observability-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-5);
  }

  .observability-header h1 {
    font-size: var(--font-size-2xl);
    font-weight: 700;
    color: var(--color-text-primary);
    margin: 0;
  }

  .observability-header p {
    margin: var(--spacing-1) 0 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .observability-nav {
    display: inline-flex;
    gap: var(--spacing-2);
    padding: var(--spacing-1);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .observability-nav a {
    padding: var(--spacing-2) var(--spacing-3);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    font-weight: 600;
    transition: background var(--duration-fast) var(--easing-default),
      color var(--duration-fast) var(--easing-default);
  }

  .observability-nav a:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }

  .observability-nav a.active {
    background: var(--color-surface-selected);
    color: var(--color-brand-primary);
  }

  @media (max-width: 900px) {
    .observability-shell {
      padding: var(--spacing-4);
    }

    .observability-header {
      flex-direction: column;
      align-items: stretch;
    }

    .observability-nav {
      width: 100%;
      justify-content: stretch;
    }

    .observability-nav a {
      flex: 1;
      text-align: center;
    }
  }
</style>
