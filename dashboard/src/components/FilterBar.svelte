<script lang="ts">
  /**
   * FilterBar — composable filter controls for audit, security, and session views.
   *
   * Emits a `filter` event with the current filter state whenever any control changes.
   * Parent components bind to `onfilter` to receive the updated filter object.
   *
   * Ref: tasks.md T-X.14, ADE_DESIGN_PLAN §5.4
   */

  interface FilterConfig {
    /** Show time range picker */
    timeRange?: boolean;
    /** Show agent selector dropdown */
    agentSelector?: boolean;
    /** Agent options: [{id, name}] */
    agents?: Array<{ id: string; name: string }>;
    /** Show event type dropdown */
    eventType?: boolean;
    /** Event type options */
    eventTypes?: string[];
    /** Show severity checkboxes */
    severity?: boolean;
    /** Show free-text search */
    search?: boolean;
    /** Placeholder for search input */
    searchPlaceholder?: string;
  }

  interface FilterState {
    from: string;
    to: string;
    agentId: string;
    eventType: string;
    severities: string[];
    query: string;
  }

  let {
    config = {} as FilterConfig,
    onfilter,
  }: {
    config: FilterConfig;
    onfilter?: (state: FilterState) => void;
  } = $props();

  let from = $state('');
  let to = $state('');
  let agentId = $state('');
  let eventType = $state('');
  let severities = $state<string[]>([]);
  let query = $state('');

  const severityLevels = ['info', 'warning', 'error', 'critical'];

  function emitFilter() {
    onfilter?.({
      from,
      to,
      agentId,
      eventType,
      severities: [...severities],
      query,
    });
  }

  function toggleSeverity(level: string) {
    if (severities.includes(level)) {
      severities = severities.filter(s => s !== level);
    } else {
      severities = [...severities, level];
    }
    emitFilter();
  }

  function clearAll() {
    from = '';
    to = '';
    agentId = '';
    eventType = '';
    severities = [];
    query = '';
    emitFilter();
  }

  let hasActiveFilters = $derived(
    from !== '' || to !== '' || agentId !== '' || eventType !== '' || severities.length > 0 || query !== ''
  );
</script>

<div class="filter-bar" role="search" aria-label="Filter controls">
  {#if config.search !== false}
    <div class="filter-group">
      <input
        type="search"
        class="search-input"
        placeholder={config.searchPlaceholder ?? 'Search…'}
        bind:value={query}
        oninput={emitFilter}
        aria-label="Search"
      />
    </div>
  {/if}

  {#if config.agentSelector}
    <div class="filter-group">
      <label class="filter-label" for="filter-agent">Agent</label>
      <select
        id="filter-agent"
        class="filter-select"
        bind:value={agentId}
        onchange={emitFilter}
      >
        <option value="">All agents</option>
        {#each config.agents ?? [] as agent}
          <option value={agent.id}>{agent.name}</option>
        {/each}
      </select>
    </div>
  {/if}

  {#if config.eventType}
    <div class="filter-group">
      <label class="filter-label" for="filter-event-type">Event type</label>
      <select
        id="filter-event-type"
        class="filter-select"
        bind:value={eventType}
        onchange={emitFilter}
      >
        <option value="">All types</option>
        {#each config.eventTypes ?? [] as et}
          <option value={et}>{et}</option>
        {/each}
      </select>
    </div>
  {/if}

  {#if config.timeRange}
    <div class="filter-group">
      <label class="filter-label" for="filter-from">From</label>
      <input
        id="filter-from"
        type="datetime-local"
        class="filter-input"
        bind:value={from}
        onchange={emitFilter}
      />
    </div>
    <div class="filter-group">
      <label class="filter-label" for="filter-to">To</label>
      <input
        id="filter-to"
        type="datetime-local"
        class="filter-input"
        bind:value={to}
        onchange={emitFilter}
      />
    </div>
  {/if}

  {#if config.severity}
    <div class="filter-group severity-group">
      <span class="filter-label">Severity</span>
      <div class="severity-checks">
        {#each severityLevels as level}
          <label class="severity-check">
            <input
              type="checkbox"
              checked={severities.includes(level)}
              onchange={() => toggleSeverity(level)}
            />
            <span class="severity-label severity-{level}">{level}</span>
          </label>
        {/each}
      </div>
    </div>
  {/if}

  {#if hasActiveFilters}
    <button class="clear-btn" onclick={clearAll} aria-label="Clear all filters">
      Clear
    </button>
  {/if}
</div>

<style>
  .filter-bar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-2);
    align-items: flex-end;
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
  }

  .filter-group {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .filter-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .search-input {
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    font-family: var(--font-family-sans);
    min-width: 180px;
    transition: border-color var(--duration-fast) var(--easing-default);
  }

  .search-input:focus {
    outline: none;
    border-color: var(--color-focus-ring);
    box-shadow: var(--shadow-focus-ring);
  }

  .search-input::placeholder {
    color: var(--color-text-disabled);
  }

  .filter-select,
  .filter-input {
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    font-family: var(--font-family-sans);
    transition: border-color var(--duration-fast) var(--easing-default);
  }

  .filter-select:focus,
  .filter-input:focus {
    outline: none;
    border-color: var(--color-focus-ring);
    box-shadow: var(--shadow-focus-ring);
  }

  .severity-group {
    flex-direction: row;
    align-items: center;
    gap: var(--spacing-2);
  }

  .severity-checks {
    display: flex;
    gap: var(--spacing-2);
  }

  .severity-check {
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
    cursor: pointer;
    font-size: var(--font-size-xs);
  }

  .severity-check input[type="checkbox"] {
    accent-color: var(--color-brand-primary);
  }

  .severity-label {
    text-transform: capitalize;
  }

  .severity-info { color: var(--color-text-secondary); }
  .severity-warning { color: var(--color-severity-soft); }
  .severity-error { color: var(--color-severity-active); }
  .severity-critical { color: var(--color-severity-hard); }

  .clear-btn {
    padding: var(--spacing-1) var(--spacing-2);
    background: transparent;
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: all var(--duration-fast) var(--easing-default);
    align-self: flex-end;
  }

  .clear-btn:hover {
    background: var(--color-surface-hover);
    color: var(--color-text-primary);
  }

  .clear-btn:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus-ring);
  }

  @media (max-width: 640px) {
    .filter-bar {
      flex-direction: column;
      align-items: stretch;
    }

    .severity-group {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>