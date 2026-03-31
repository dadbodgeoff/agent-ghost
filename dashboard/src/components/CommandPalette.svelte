<script lang="ts">
  /**
   * CommandPalette — Enhanced Cmd+K global search overlay (Phase 2, Task 3.1).
   *
   * Features:
   * - Prefixed scoped search: > commands, @ agents, # sessions, / settings
   * - Frecency ranking (frequency x recency)
   * - Agent-specific commands (start, pause, logs, kill-all)
   * - Keyboard shortcut display inline
   *
   * Ref: T-3.13.2
   */
    import { goto } from '$app/navigation';
    import { getGhostClient } from '$lib/ghost-client';
    import { hrefForSearchResult } from '$lib/search/navigation';
    import { authSessionStore } from '$lib/stores/auth-session.svelte';
    import { agentsStore, type Agent } from '$lib/stores/agents.svelte';
    import { frecencyTracker } from '$lib/frecency';
    import { shortcuts } from '$lib/shortcuts';
  import type { SearchResult } from '@ghost/sdk';

  type SearchPrefix = '>' | '@' | '#' | '/';

  interface PaletteCommand {
    id: string;
    label: string;
    category: 'command' | 'agent' | 'session' | 'setting';
    shortcut?: string;
    action: () => void | Promise<void>;
    frecencyScore: number;
  }

  let open = $state(false);
  let query = $state('');
  let results: SearchResult[] = $state([]);
  let paletteCommands: PaletteCommand[] = $state([]);
  let loading = $state(false);
  let selectedIndex = $state(0);
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;
  let mode = $state<'search' | 'commands'>('search');
  let inputEl = $state<HTMLInputElement | null>(null);

  // Static commands
  const STATIC_COMMANDS: PaletteCommand[] = [
    { id: 'nav-overview', label: 'Go to Overview', category: 'command', action: () => goto('/'), frecencyScore: 0 },
    { id: 'nav-convergence', label: 'Go to Convergence', category: 'command', action: () => goto('/convergence'), frecencyScore: 0 },
    { id: 'nav-memory', label: 'Go to Memory', category: 'command', action: () => goto('/memory'), frecencyScore: 0 },
    { id: 'nav-goals', label: 'Go to Proposals', category: 'command', action: () => goto('/goals'), frecencyScore: 0 },
    { id: 'nav-sessions', label: 'Go to Sessions', category: 'command', action: () => goto('/sessions'), frecencyScore: 0 },
    { id: 'nav-agents', label: 'Go to Agents', category: 'command', action: () => goto('/agents'), frecencyScore: 0 },
    { id: 'nav-studio', label: 'Go to Studio', category: 'command', action: () => goto('/studio'), frecencyScore: 0 },
    { id: 'nav-security', label: 'Go to Security', category: 'command', action: () => goto('/security'), frecencyScore: 0 },
    { id: 'nav-costs', label: 'Go to Costs', category: 'command', action: () => goto('/costs'), frecencyScore: 0 },
    { id: 'nav-settings', label: 'Go to Settings', category: 'command', action: () => goto('/settings'), frecencyScore: 0 },
    { id: 'nav-workflows', label: 'Go to Workflows', category: 'command', action: () => goto('/workflows'), frecencyScore: 0 },
    { id: 'nav-skills', label: 'Go to Skills', category: 'command', action: () => goto('/skills'), frecencyScore: 0 },
    { id: 'theme-toggle', label: 'Toggle Theme', category: 'setting', shortcut: shortcuts.getShortcutDisplay('theme.toggle'), action: () => {
      if (typeof document === 'undefined') return;
      document.documentElement.classList.toggle('light');
      const isLight = document.documentElement.classList.contains('light');
      if (typeof localStorage !== 'undefined') {
        try {
          localStorage.setItem('ghost-theme', isLight ? 'light' : 'dark');
        } catch {
          // Ignore storage persistence failures and still apply the theme.
        }
      }
    }, frecencyScore: 0 },
    { id: 'search-global', label: 'Global Search', category: 'command', shortcut: shortcuts.getShortcutDisplay('search.global'), action: () => goto('/search'), frecencyScore: 0 },
    { id: 'new-session', label: 'New Studio Session', category: 'command', shortcut: shortcuts.getShortcutDisplay('studio.newSession'), action: () => goto('/studio'), frecencyScore: 0 },
    { id: 'nav-providers', label: 'Go to Providers', category: 'setting', action: () => goto('/settings/providers'), frecencyScore: 0 },
    { id: 'nav-channels', label: 'Go to Channels', category: 'setting', action: () => goto('/channels'), frecencyScore: 0 },
    { id: 'nav-webhooks', label: 'Go to Webhooks', category: 'setting', action: () => goto('/settings/webhooks'), frecencyScore: 0 },
    { id: 'nav-oauth', label: 'Go to OAuth Settings', category: 'setting', action: () => goto('/settings/oauth'), frecencyScore: 0 },
    { id: 'nav-backups', label: 'Go to Backups', category: 'setting', action: () => goto('/settings/backups'), frecencyScore: 0 },
  ];

  function parseQuery(raw: string): { prefix: SearchPrefix | null; query: string } {
    const prefixes: SearchPrefix[] = ['>', '@', '#', '/'];
    for (const p of prefixes) {
      if (raw.startsWith(p)) {
        return { prefix: p, query: raw.slice(1).trim() };
      }
    }
    return { prefix: null, query: raw };
  }

  function buildAgentCommands(agents: Agent[]): PaletteCommand[] {
    const commands: PaletteCommand[] = [];
    for (const agent of agents) {
      commands.push(
        {
          id: `pause-${agent.id}`,
          label: `Pause Agent: ${agent.name}`,
          category: 'agent',
          action: async () => {
            const client = await getGhostClient();
            await client.safety.pause(agent.id, 'Paused via command palette');
          },
          frecencyScore: 0,
        },
        {
          id: `logs-${agent.id}`,
          label: `Open Agent Logs: ${agent.name}`,
          category: 'agent',
          action: () => goto(`/agents/${agent.id}`),
          frecencyScore: 0,
        },
      );
    }
    if (authSessionStore.canTriggerKillAll) {
      commands.push({
        id: 'kill-all',
        label: 'Kill All Agents',
        category: 'command',
        shortcut: shortcuts.getShortcutDisplay('killSwitch.activateAll'),
        action: async () => {
          if (confirm('Kill all agents? This cannot be undone.')) {
            const client = await getGhostClient();
            await client.safety.killAll('Manual kill via command palette', 'dashboard_command_palette');
          }
        },
        frecencyScore: 0,
      });
    }
    return commands;
  }

  function fuzzyMatch(items: PaletteCommand[], query: string): PaletteCommand[] {
    const lower = query.toLowerCase();
    return items.filter(item => {
      const label = item.label.toLowerCase();
      // Simple fuzzy: check if all chars in query appear in order in label
      let qi = 0;
      for (let i = 0; i < label.length && qi < lower.length; i++) {
        if (label[i] === lower[qi]) qi++;
      }
      return qi === lower.length;
    });
  }

  function filterCommands(
    commands: PaletteCommand[],
    prefix: SearchPrefix | null,
    queryStr: string,
  ): PaletteCommand[] {
    const categoryMap: Record<SearchPrefix, string> = {
      '>': 'command',
      '@': 'agent',
      '#': 'session',
      '/': 'setting',
    };

    let filtered = commands;
    if (prefix) {
      filtered = filtered.filter(c => c.category === categoryMap[prefix]);
    }
    if (queryStr) {
      filtered = fuzzyMatch(filtered, queryStr);
    }

    // Update frecency scores
    for (const cmd of filtered) {
      cmd.frecencyScore = frecencyTracker.score(cmd.id);
    }

    return filtered.sort((a, b) => b.frecencyScore - a.frecencyScore);
  }

  function getAllCommands(): PaletteCommand[] {
    return [...STATIC_COMMANDS, ...buildAgentCommands(agentsStore.list)];
  }

  function handleGlobalKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault();
      open = !open;
      if (open) {
        query = '';
        results = [];
        selectedIndex = 0;
        // Show recent commands on open
        const allCmds = getAllCommands();
        const recentIds = frecencyTracker.getRecent(8);
        if (recentIds.length > 0) {
          paletteCommands = recentIds
            .map(id => allCmds.find(c => c.id === id))
            .filter((c): c is PaletteCommand => !!c);
        } else {
          paletteCommands = allCmds.slice(0, 8);
        }
        mode = 'commands';
      }
    }
    if (e.key === 'Escape' && open) {
      open = false;
    }
  }

  $effect(() => {
    if (open && inputEl) {
      requestAnimationFrame(() => inputEl?.focus());
    }
  });

  function handleInput() {
    if (debounceTimer) clearTimeout(debounceTimer);

    const { prefix, query: queryStr } = parseQuery(query);

    // If prefix is used or query starts with prefix chars, show commands
    if (prefix) {
      mode = 'commands';
      paletteCommands = filterCommands(getAllCommands(), prefix, queryStr);
      selectedIndex = 0;
      return;
    }

    if (!query.trim()) {
      // Show recent commands when empty
      const allCmds = getAllCommands();
      const recentIds = frecencyTracker.getRecent(8);
      if (recentIds.length > 0) {
        paletteCommands = recentIds
          .map(id => allCmds.find(c => c.id === id))
          .filter((c): c is PaletteCommand => !!c);
      } else {
        paletteCommands = allCmds.slice(0, 8);
      }
      mode = 'commands';
      results = [];
      return;
    }

    // Also check against commands without prefix
    paletteCommands = filterCommands(getAllCommands(), null, queryStr);
    mode = paletteCommands.length > 0 ? 'commands' : 'search';

    // Also search via API
    debounceTimer = setTimeout(search, 200);
  }

  async function search() {
    if (!query.trim()) return;
    loading = true;
    try {
      const client = await getGhostClient();
      const res = await client.search.query({ q: query.trim(), limit: 10 });
      results = res.results ?? [];
      // If no command matches but search results exist, switch to search mode
      if (paletteCommands.length === 0 && results.length > 0) {
        mode = 'search';
      }
      selectedIndex = 0;
    } catch {
      results = [];
    } finally {
      loading = false;
    }
  }

  function getDisplayItems(): Array<{ type: 'command'; item: PaletteCommand } | { type: 'result'; item: SearchResult }> {
    const items: Array<{ type: 'command'; item: PaletteCommand } | { type: 'result'; item: SearchResult }> = [];
    for (const cmd of paletteCommands) {
      items.push({ type: 'command', item: cmd });
    }
    for (const r of results) {
      items.push({ type: 'result', item: r });
    }
    return items;
  }

  let displayItems = $derived(getDisplayItems());

  function handleKeydown(e: KeyboardEvent) {
    const total = displayItems.length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, total - 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const item = displayItems[selectedIndex];
      if (item) {
        if (item.type === 'command') {
          frecencyTracker.record(item.item.id);
          item.item.action();
          open = false;
        } else {
          const r = item.item;
          const link = hrefForSearchResult(r, query.trim());
          goto(link);
          open = false;
        }
      } else if (query.trim()) {
        goto(`/search?q=${encodeURIComponent(query.trim())}`);
        open = false;
      }
    }
  }

  function categoryLabel(prefix: SearchPrefix | null): string {
    if (!prefix) return '';
    const labels: Record<SearchPrefix, string> = {
      '>': 'Commands',
      '@': 'Agents',
      '#': 'Sessions',
      '/': 'Settings',
    };
    return labels[prefix];
  }

  let currentPrefix = $derived(parseQuery(query).prefix);
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

{#if open}
  <div class="overlay" onclick={() => open = false} role="presentation">
    <div class="palette" role="dialog" tabindex="-1" aria-modal="true" aria-label="Command Palette" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
      <div class="input-row">
        <input
          type="text"
          bind:this={inputEl}
          bind:value={query}
          oninput={handleInput}
          onkeydown={handleKeydown}
          placeholder="Search or type > for commands, @ for agents, # sessions, / settings"
          class="palette-input"
        />
        <kbd class="shortcut">ESC</kbd>
      </div>

      {#if currentPrefix}
        <div class="scope-label">{categoryLabel(currentPrefix)}</div>
      {/if}

      {#if loading}
        <p class="hint">Searching...</p>
      {:else if displayItems.length > 0}
        <ul class="result-list" role="listbox">
          {#each displayItems as item, i}
            <li>
              <button
                type="button"
                class="result-item"
                class:selected={i === selectedIndex}
                role="option"
                aria-selected={i === selectedIndex}
                onclick={() => {
                  if (item.type === 'command') {
                    frecencyTracker.record(item.item.id);
                    item.item.action();
                  } else {
                    const r = item.item;
                    const link = hrefForSearchResult(r, query.trim());
                    goto(link);
                  }
                  open = false;
                }}
              >
              {#if item.type === 'command'}
                <span class="result-type cmd-type">{item.item.category}</span>
                <span class="result-title">{item.item.label}</span>
                {#if item.item.shortcut}
                  <kbd class="inline-shortcut">{item.item.shortcut}</kbd>
                {/if}
              {:else}
                <span class="result-type">{item.item.result_type}</span>
                <span class="result-title">{item.item.title}</span>
                {#if item.item.snippet}
                  <span class="result-snippet">{item.item.snippet.slice(0, 60)}</span>
                {/if}
              {/if}
              </button>
            </li>
          {/each}
        </ul>
      {:else if query.trim()}
        <p class="hint">No results. Press Enter to search.</p>
      {:else}
        <p class="hint">Type to search or use prefix: &gt; commands, @ agents, # sessions, / settings</p>
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
    width: 600px;
    max-height: 450px;
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

  .scope-label {
    padding: var(--spacing-1) var(--spacing-3);
    font-size: var(--font-size-xs);
    color: var(--color-interactive-primary);
    font-weight: var(--font-weight-semibold);
    border-bottom: 1px solid var(--color-border-subtle);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .result-list {
    list-style: none;
    padding: 0;
    margin: 0;
    max-height: 350px;
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

  .cmd-type {
    background: color-mix(in srgb, var(--color-interactive-primary) 15%, transparent);
    color: var(--color-interactive-primary);
  }

  .result-title {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-weight: 500;
    flex: 1;
  }

  .result-snippet {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .inline-shortcut {
    font-size: 10px;
    padding: 1px 5px;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
    margin-left: auto;
  }

  .hint {
    padding: var(--spacing-3);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
</style>
