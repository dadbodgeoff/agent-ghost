<script lang="ts">
  /**
   * LoadMorePagination — cursor-based pagination trigger (Phase 2, Task 3.7).
   *
   * Shows a "Load more" button when there are more items to fetch.
   */
  let { loadMore, hasMore, loading, totalCount }: {
    loadMore: () => Promise<void>;
    hasMore: boolean;
    loading: boolean;
    totalCount?: number;
  } = $props();
</script>

{#if hasMore}
  <div class="pagination-trigger">
    <button class="load-more-btn" type="button" onclick={loadMore} disabled={loading}>
      {loading ? 'Loading...' : 'Load more'}
    </button>
    {#if totalCount !== undefined}
      <span class="total-hint">({totalCount} total)</span>
    {/if}
  </div>
{/if}

<style>
  .pagination-trigger {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-2);
    padding: var(--spacing-3);
  }

  .load-more-btn {
    padding: var(--spacing-2) var(--spacing-6);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background 0.1s;
  }
  .load-more-btn:hover:not(:disabled) {
    background: var(--color-surface-hover);
  }
  .load-more-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .total-hint {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
</style>
