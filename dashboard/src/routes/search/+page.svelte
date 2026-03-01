<script lang="ts">
  /**
   * Unified search results page.
   * Results grouped by entity type.
   *
   * Ref: T-3.13.1
   */
  import { page } from '$app/stores';
  import { api } from '$lib/api';

  interface SearchResult {
    result_type: string;
    id: string;
    title: string;
    snippet: string;
    score: number;
  }

  let query = $state('');
  let results: SearchResult[] = $state([]);
  let total = $state(0);
  let loading = $state(false);
  let error: string | null = $state(null);
  let searched = $state(false);

  // Pick up query from URL params.
  $effect(() => {
    const q = $page.url.searchParams.get('q');
    if (q && q !== query) {
      query = q;
      doSearch();
    }
  });

  async function doSearch() {
    if (!query.trim()) return;
    loading = true;
    error = null;
    searched = true;
    try {
      const res = await api.get(`/api/search?q=${encodeURIComponent(query.trim())}`);
      results = res.results ?? [];
      total = res.total ?? 0;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Search failed';
      results = [];
    } finally {
      loading = false;
    }
  }

  function handleSubmit(e: Event) {
    e.preventDefault();
    doSearch();
  }

  const TYPE_LABELS: Record<string, string> = {
    agent: 'Agents',
    session: 'Sessions',
    memory: 'Memories',
    proposal: 'Proposals',
    audit: 'Audit Log',
  };

  const TYPE_LINKS: Record<string, (id: string) => string> = {
    agent: (id) => `/agents/${id}`,
    session: (id) => `/sessions/${id}`,
    memory: (id) => `/memory`,
    proposal: (id) => `/goals/${id}`,
    audit: (id) => `/security`,
  };

  let groupedResults = $derived.by(() => {
    const groups: Record<string, SearchResult[]> = {};
    for (const r of results) {
      if (!groups[r.result_type]) groups[r.result_type] = [];
      groups[r.result_type].push(r);
    }
    return groups;
  });
</script>

<svelte:head>
  <title>Search | ADE</title>
</svelte:head>

<div class="search-page">
  <header class="page-header">
    <h1>Search</h1>
  </header>

  <form class="search-form" onsubmit={handleSubmit}>
    <input
      type="text"
      bind:value={query}
      placeholder="Search agents, sessions, memories, proposals, audit…"
      class="search-input"
      aria-label="Search"
    />
    <button type="submit" class="search-btn" disabled={loading}>
      {loading ? 'Searching…' : 'Search'}
    </button>
  </form>

  {#if error}
    <p class="error-msg">{error}</p>
  {/if}

  {#if searched && !loading}
    <p class="result-count">{total} result{total !== 1 ? 's' : ''} for "{query}"</p>

    {#each Object.entries(groupedResults) as [type, items]}
      <section class="result-group">
        <h2>{TYPE_LABELS[type] ?? type}</h2>
        <ul class="result-list">
          {#each items as item}
            <li class="result-item">
              <a href={TYPE_LINKS[type]?.(item.id) ?? '#'} class="result-link">
                <span class="result-title">{item.title}</span>
                <span class="result-id mono">{item.id.slice(0, 8)}…</span>
              </a>
              {#if item.snippet}
                <p class="result-snippet">{item.snippet}</p>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/each}

    {#if results.length === 0}
      <p class="no-results">No results found. Try a different query.</p>
    {/if}
  {/if}
</div>

<style>
  .search-page { padding: var(--spacing-6); max-width: 800px; }
  .page-header { margin-bottom: var(--spacing-4); }
  .page-header h1 { font-size: var(--font-size-2xl); font-weight: 700; color: var(--color-text-primary); }

  .search-form { display: flex; gap: var(--spacing-2); margin-bottom: var(--spacing-4); }
  .search-input {
    flex: 1;
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-3);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }
  .search-input:focus { outline: 2px solid var(--color-interactive-primary); outline-offset: -1px; }

  .search-btn {
    background: var(--color-interactive-primary);
    color: var(--color-text-inverse);
    border: none;
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-4);
    cursor: pointer;
    font-size: var(--font-size-sm);
  }
  .search-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .result-count { font-size: var(--font-size-sm); color: var(--color-text-muted); margin-bottom: var(--spacing-4); }

  .result-group { margin-bottom: var(--spacing-4); }
  .result-group h2 { font-size: var(--font-size-sm); font-weight: 600; color: var(--color-text-muted); text-transform: uppercase; margin-bottom: var(--spacing-2); letter-spacing: 0.05em; }

  .result-list { list-style: none; padding: 0; margin: 0; }
  .result-item {
    padding: var(--spacing-2) var(--spacing-3);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    margin-bottom: var(--spacing-1);
    background: var(--color-bg-elevated-1);
  }
  .result-item:hover { border-color: var(--color-border-default); }

  .result-link { display: flex; justify-content: space-between; align-items: center; text-decoration: none; color: var(--color-text-primary); }
  .result-title { font-weight: 500; font-size: var(--font-size-sm); }
  .result-id { font-size: var(--font-size-xs); color: var(--color-text-muted); }
  .result-snippet { font-size: var(--font-size-xs); color: var(--color-text-muted); margin-top: var(--spacing-1); }

  .no-results { text-align: center; padding: var(--spacing-8); color: var(--color-text-muted); }
  .error-msg { color: var(--color-severity-hard); font-size: var(--font-size-sm); padding: var(--spacing-3); background: var(--color-bg-elevated-1); border: 1px solid var(--color-severity-hard); border-radius: var(--radius-sm); margin-bottom: var(--spacing-3); }
  .mono { font-family: var(--font-family-mono); font-variant-numeric: tabular-nums; }
</style>
