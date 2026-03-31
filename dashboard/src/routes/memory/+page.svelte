<script lang="ts">
  /**
   * Memory Browser — search + filter memories (T-2.3.1).
   * Supports text search, type/importance/confidence filtering.
   */
  import { page } from '$app/stores';
  import { onDestroy, onMount } from 'svelte';
  import { tick } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import type { MemoryEntry, MemorySearchResultEntry } from '@ghost/sdk';
  import { wsStore, type WsMessage } from '$lib/stores/websocket.svelte';
  import MemoryCard from '../../components/MemoryCard.svelte';

  interface MemoryCardItem {
    memory_id: string;
    snapshot: string;
    created_at: string;
  }

  let memories: MemoryCardItem[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let searchQuery = $state('');
  let agentFilter = $state('');
  let typeFilter = $state('');
  let importanceFilter = $state('');
  let isSearchMode = $state(false);
  let focusedMemoryId = $state('');
  let mounted = $state(false);
  let lastUrlState = $state('');
  let unsubs: Array<() => void> = [];

  const memoryTypeOptions = [
    { value: '', label: 'All Types' },
    { value: 'Episodic', label: 'Episodic' },
    { value: 'Semantic', label: 'Semantic' },
    { value: 'Procedural', label: 'Procedural' },
    { value: 'Conversation', label: 'Conversation' },
    { value: 'Goal', label: 'Goal' },
    { value: 'AgentGoal', label: 'Agent Goal' },
    { value: 'AgentReflection', label: 'Agent Reflection' },
  ];

  const importanceOptions = [
    { value: '', label: 'All Importance' },
    { value: 'Critical', label: 'Critical' },
    { value: 'High', label: 'High' },
    { value: 'Normal', label: 'Normal' },
    { value: 'Low', label: 'Low' },
    { value: 'Trivial', label: 'Trivial' },
  ];

  onMount(async () => {
    mounted = true;
    lastUrlState = $page.url.search;
    unsubs = [
      wsStore.on('ProposalDecision', (_msg: WsMessage) => {
        void refreshForCurrentMode();
      }),
      wsStore.onResync(() => {
        void refreshForCurrentMode();
      }),
    ];
    await hydrateFromUrl();
  });

  onDestroy(() => {
    for (const unsub of unsubs) {
      unsub();
    }
  });

  async function loadMemories() {
    try {
      loading = true;
      error = '';
      const client = await getGhostClient();
      const data = await client.memory.list();
      memories = (data.memories ?? []).map(toMemoryCardItem);
      isSearchMode = false;
      await focusMemory();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load memories';
    } finally {
      loading = false;
    }
  }

  async function handleSearch() {
    if (!searchQuery.trim() && !agentFilter && !typeFilter && !importanceFilter) {
      await loadMemories();
      return;
    }

    try {
      loading = true;
      error = '';
      const client = await getGhostClient();
      const data = await client.memory.search({
        q: searchQuery.trim() || undefined,
        agent_id: agentFilter || undefined,
        memory_type: typeFilter || undefined,
        importance: importanceFilter || undefined,
        limit: 100,
      });
      memories = (data.results ?? []).map(toSearchMemoryCardItem);
      isSearchMode = true;
      await focusMemory();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Search failed';
    } finally {
      loading = false;
    }
  }

  function clearSearch() {
    searchQuery = '';
    agentFilter = '';
    typeFilter = '';
    importanceFilter = '';
    focusedMemoryId = '';
    void loadMemories();
  }

  $effect(() => {
    if (!mounted) return;
    const urlState = $page.url.search;
    if (urlState === lastUrlState) return;
    lastUrlState = urlState;
    void hydrateFromUrl();
  });

  async function hydrateFromUrl() {
    const q = ($page.url.searchParams.get('q') ?? '').trim();
    const agent = ($page.url.searchParams.get('agent_id') ?? '').trim();
    const memoryType = ($page.url.searchParams.get('memory_type') ?? '').trim();
    const importance = ($page.url.searchParams.get('importance') ?? '').trim();
    focusedMemoryId = ($page.url.searchParams.get('focus') ?? '').trim();

    searchQuery = q;
    agentFilter = agent;
    typeFilter = memoryType;
    importanceFilter = importance;

    if (q || agent || memoryType || importance) {
      await handleSearch();
    } else {
      await loadMemories();
    }
  }

  async function refreshForCurrentMode() {
    if (searchQuery.trim() || agentFilter || typeFilter || importanceFilter) {
      await handleSearch();
    } else {
      await loadMemories();
    }
  }

  async function focusMemory() {
    if (!focusedMemoryId) return;
    await tick();
    const target = document.getElementById(`memory-${focusedMemoryId}`);
    target?.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }

  function toMemoryCardItem(memory: MemoryEntry): MemoryCardItem {
    return {
      memory_id: memory.memory_id,
      snapshot: typeof memory.snapshot === 'string' ? memory.snapshot : JSON.stringify(memory.snapshot),
      created_at: memory.created_at,
    };
  }

  function toSearchMemoryCardItem(memory: MemorySearchResultEntry): MemoryCardItem {
    return {
      memory_id: memory.memory_id,
      snapshot: typeof memory.snapshot === 'string' ? memory.snapshot : JSON.stringify(memory.snapshot),
      created_at: memory.created_at,
    };
  }
</script>

<h1 class="page-title">Memory</h1>

<!-- Search Bar -->
<div class="search-section">
  <div class="search-row">
    <input
      type="text"
      class="search-input"
      placeholder="Search memories..."
      aria-label="Search memories"
      bind:value={searchQuery}
      onkeydown={(e) => { if (e.key === 'Enter') handleSearch(); }}
    />
    <button class="btn-search" onclick={handleSearch}>Search</button>
    {#if isSearchMode}
      <button class="btn-clear" onclick={clearSearch}>Clear</button>
    {/if}
  </div>

  <div class="filter-row">
    <input type="text" class="filter-input" placeholder="Agent ID" bind:value={agentFilter} />
    <select class="filter-select" bind:value={typeFilter}>
      {#each memoryTypeOptions as option}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
    <select class="filter-select" bind:value={importanceFilter}>
      {#each importanceOptions as option}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </div>
</div>

{#if isSearchMode}
  <p class="result-count">{memories.length} result{memories.length !== 1 ? 's' : ''} found</p>
{/if}

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={loadMemories}>Retry</button>
  </div>
{:else if memories.length === 0}
  <div class="empty-state">
    <p>{isSearchMode ? 'No memories match your search.' : 'No memories stored yet.'}</p>
  </div>
{:else}
  <div class="memory-list">
    {#each memories as mem (mem.memory_id)}
      <a class="memory-link" href={`/memory/${mem.memory_id}`}>
        <MemoryCard
          memory_id={mem.memory_id}
          snapshot={mem.snapshot}
          created_at={mem.created_at}
          focused={mem.memory_id === focusedMemoryId}
        />
      </a>
    {/each}
  </div>
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-4);
  }

  .search-section {
    margin-bottom: var(--spacing-4);
  }

  .search-row {
    display: flex;
    gap: var(--spacing-2);
    margin-bottom: var(--spacing-2);
  }

  .search-input {
    flex: 1;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .search-input::placeholder { color: var(--color-text-disabled); }

  .btn-search, .btn-clear {
    padding: var(--spacing-2) var(--spacing-4);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .btn-search {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
  }

  .btn-clear {
    background: var(--color-bg-elevated-2);
    color: var(--color-text-secondary);
  }

  .filter-row {
    display: flex;
    gap: var(--spacing-2);
    flex-wrap: wrap;
  }

  .filter-input, .filter-select {
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    color: var(--color-text-primary);
  }

  .filter-input { max-width: 160px; }
  .filter-input::placeholder { color: var(--color-text-disabled); }

  .result-count {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-3);
  }

  .memory-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .memory-link {
    text-decoration: none;
    color: inherit;
  }

  .skeleton-block {
    height: 200px;
    background: var(--color-bg-elevated-1);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }

  .empty-state, .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .error-state button {
    margin-top: var(--spacing-4);
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
  }

  @media (max-width: 640px) {
    .filter-row { flex-direction: column; }
    .filter-input { max-width: 100%; }
  }
</style>
