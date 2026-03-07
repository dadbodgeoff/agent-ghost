<script lang="ts">
  /**
   * Breadcrumb — Route-based breadcrumb navigation (Phase 2, Task 3.9).
   *
   * Derives breadcrumbs from current URL path. UUIDs are truncated.
   * Last segment shows as non-linked current page.
   */
  import { page } from '$app/stores';

  interface Crumb {
    label: string;
    href: string;
  }

  let crumbs = $derived.by((): Crumb[] => {
    const path = $page.url.pathname;
    const segments = path.split('/').filter(Boolean);
    const result: Crumb[] = [{ label: 'Home', href: '/' }];

    let accumulated = '';
    for (const segment of segments) {
      accumulated += '/' + segment;
      result.push({
        label: formatSegment(segment),
        href: accumulated,
      });
    }
    return result;
  });

  function formatSegment(s: string): string {
    // UUID-like string: truncate for readability
    if (/^[0-9a-f-]{36}$/.test(s)) return s.slice(0, 8) + '...';
    // kebab-case → Title Case
    return s
      .split('-')
      .map(w => w.charAt(0).toUpperCase() + w.slice(1))
      .join(' ');
  }
</script>

{#if crumbs.length > 1}
<nav aria-label="Breadcrumb" class="breadcrumb-bar">
  <ol>
    {#each crumbs as crumb, i}
      <li>
        {#if i < crumbs.length - 1}
          <a href={crumb.href}>{crumb.label}</a>
          <span class="separator" aria-hidden="true">/</span>
        {:else}
          <span aria-current="page">{crumb.label}</span>
        {/if}
      </li>
    {/each}
  </ol>
</nav>
{/if}

<style>
  .breadcrumb-bar {
    padding: var(--spacing-1) var(--spacing-4);
    background: var(--color-bg-elevated-1);
    border-bottom: 1px solid var(--color-border-subtle);
    flex-shrink: 0;
  }

  ol {
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li {
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
  }

  a {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-decoration: none;
    transition: color 0.1s;
  }

  a:hover {
    color: var(--color-interactive-primary);
  }

  .separator {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    opacity: 0.5;
  }

  span[aria-current="page"] {
    font-size: var(--font-size-xs);
    color: var(--color-text-primary);
    font-weight: var(--font-weight-medium);
  }
</style>
