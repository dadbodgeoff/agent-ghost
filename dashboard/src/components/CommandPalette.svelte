<script lang="ts">
  /**
   * CommandPalette — Cmd+K global search overlay.
   * Inline results dropdown, Enter navigates to /search.
   *
   * Ref: T-3.13.2
   */
  import { goto } from '$app/navigation';
  import { api } from '$lib/api';

  interface SearchResult {
    result_type: string;
    id: string;
    title: string;
    snippet: string;
    score: number;
  }

  let open = $state(false);
  let query = $state('');
  let results: SearchResult[] = $state([]);
  let loading = $state(false);
  let selectedIndex = $state(0);
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  const TYPE_LINKS: Record<string, (id: string) => string> = {
    agent: (id) => `/agents/${id}`,
    session: (id) => `/sessions/${id}`,
    memory: (_id) => `/memory`,
    proposal: (id) => `/goals/${id}`,
    audit: (_id) => `/security`,
    skill: (_id) => `/skills`,
    webhook: (_id) => `/settings/webhooks`,
    notification: (_id) => `/settings/notifications`,
  };

  function handleGlobalKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      open = !open;
      if (open) {
        query = '';
        results = [];
        selectedIndex = 0;
      }
    }
    if (e.key === 'Escape' && open) {
      open = false;
    }
  }

  function handleInput() {
    if (debounceTimer) clearTimeout(debounceTimer);
    if (!query.trim()) {
      results = [];
      return;
    }
    debounceTimer = setTimeout(search, 200);
  }

  async function search() {
    if (!query.trim()) return;
    loading = true;
    try {
      const res = await api.get(`/api/search?q=${encodeURIComponent(query.trim())}&limit=10`);
      results = res.results ?? [];
      selectedIndex = 0;
    } catch {
      results = [];
    } finally {
      loading = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, results.length - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (results.length > 0 && results[selectedIndex]) {
        const r = results[selectedIndex];
        const link = TYPE_LINKS[r.result_type]?.(r.id) ?? `/search?q=${encodeURIComponent(query)}`;
        goto(link);
        open = false;
      } else if (query.trim()) {
        goto(`/search?q=${encodeURIComponent(query.trim())}`);
        open = false;
      }
    }
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

{#if open}
  <div class="overlay" onclick={() => open = false} role="presentation">
    <div class="palette" role="dialog" aria-modal="true" aria-label="Search" onclick={(e) => e.stopPropagation()}>
      <div class="input-row">
        <input
          type="text"
          bind:value={query}
          oninput={handleInput}
          onkeydown={handleKeydown}
          placeholder="Search everything…"
          class="palette-input"
          autofocus
        />
        <kbd class="shortcut">ESC</kbd>
      </div>

      {#if loading}
        <p class="hint">Searching…</p>
      {:else if results.length > 0}
        <ul class="result-list" role="listbox">
          {#each results as r, i}
            <li
              class="result-item"
              class:selected={i === selectedIndex}
              role="option"
              aria-selected={i === selectedIndex}
              onclick={() => { const link = TYPE_LINKS[r.result_type]?.(r.id) ?? '/search'; goto(link); open = false; }}
            >
              <span class="result-type">{r.result_type}</span>
              <span class="result-title">{r.title}</span>
              {#if r.snippet}
                <span class="result-snippet">{r.snippet.slice(0, 60)}</span>
              {/if}
            </li>
          {/each}
        </ul>
      {:else if query.trim()}
        <p class="hint">No results. Press Enter to search.</p>
      {:else}
        <p class="hint">Type to search agents, sessions, memories, skills…</p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--color-bg-overlay);
    z-index: 1000;
    display: flex;
    justify-content: center;
    padding-top: 120px;
  }

  .palette {
    width: 560px;
    max-height: 400px;
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-elevated-3);
    overflow: hidden;
  }

  .input-row {
    display: flex;
    align-items: center;
    padding: var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .palette-input {
    flex: 1;
    background: none;
    border: none;
    color: var(--color-text-primary);
    font-size: var(--font-size-md);
    outline: none;
  }

  .shortcut {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: 1px 6px;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
  }

  .result-list {
    list-style: none;
    padding: 0;
    margin: 0;
    max-height: 300px;
    overflow-y: auto;
  }

  .result-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    cursor: pointer;
    transition: background 0.05s;
  }

  .result-item:hover, .result-item.selected {
    background: var(--color-bg-elevated-2);
  }

  .result-type {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    background: var(--color-bg-elevated-2);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    text-transform: uppercase;
    min-width: 60px;
    text-align: center;
  }

  .result-title {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-weight: 500;
  }

  .result-snippet {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    padding: var(--spacing-3);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
</style>
