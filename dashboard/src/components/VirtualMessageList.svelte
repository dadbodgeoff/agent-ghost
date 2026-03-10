<script lang="ts">
  /**
   * VirtualMessageList — Virtual scrolling for chat messages (Phase 2, Task 3.8).
   *
   * Only renders visible messages + overscan buffer for smooth scrolling
   * at 500+ messages without DOM bloat.
   */
  import type { StudioMessage } from '$lib/stores/studioChat.svelte';
  import type { Snippet } from 'svelte';

  let {
    messages,
    containerHeight,
    estimatedItemHeight = 120,
    overscan = 5,
    autoScroll = true,
    children,
  }: {
    messages: StudioMessage[];
    containerHeight: number;
    estimatedItemHeight?: number;
    overscan?: number;
    autoScroll?: boolean;
    children: Snippet<[{ message: StudioMessage }]>;
  } = $props();

  let scrollTop = $state(0);
  let container: HTMLDivElement;
  let isUserScrolledUp = $state(false);
  let heightFlushFrame = 0;
  let autoScrollFrame = 0;

  // Measured heights (updated after render).
  // Use a plain Map + a $state version counter to avoid creating a new Map
  // on every ResizeObserver callback (which caused an infinite loop:
  // new Map → totalHeight changes → container resizes → ResizeObserver fires → repeat).
  const heightMap = new Map<string, number>();
  let heightVersion = $state(0);

  function scheduleHeightFlush() {
    if (heightFlushFrame) return;
    heightFlushFrame = requestAnimationFrame(() => {
      heightFlushFrame = 0;
      heightVersion++;
    });
  }

  function scheduleAutoScroll() {
    if (autoScrollFrame) return;
    autoScrollFrame = requestAnimationFrame(() => {
      autoScrollFrame = 0;
      container?.scrollTo({ top: container.scrollHeight });
    });
  }

  // WP5-A: Prune heightMap entries not in current message list (e.g. after session switch).
  $effect(() => {
    const currentIds = new Set(messages.map(m => m.id));
    let pruned = 0;
    for (const key of heightMap.keys()) {
      if (!currentIds.has(key)) {
        heightMap.delete(key);
        pruned++;
      }
    }
    if (pruned > 0) {
      scheduleHeightFlush();
    }
  });

  // Compute visible range
  let visibleRange = $derived.by(() => {
    const _v = heightVersion;
    let accumulatedHeight = 0;
    let startIdx = 0;
    let endIdx = messages.length;

    // Find start index
    for (let i = 0; i < messages.length; i++) {
      const h = heightMap.get(messages[i].id) ?? estimatedItemHeight;
      if (accumulatedHeight + h > scrollTop) {
        startIdx = Math.max(0, i - overscan);
        break;
      }
      accumulatedHeight += h;
    }

    // Find end index
    accumulatedHeight = 0;
    for (let i = startIdx; i < messages.length; i++) {
      const h = heightMap.get(messages[i].id) ?? estimatedItemHeight;
      accumulatedHeight += h;
      if (accumulatedHeight > containerHeight + (overscan * estimatedItemHeight)) {
        endIdx = i + 1;
        break;
      }
    }

    return { start: startIdx, end: endIdx };
  });

  // Total height for scroll container
  let totalHeight = $derived.by(() => {
    const _v = heightVersion;
    return messages.reduce((sum, m) =>
      sum + (heightMap.get(m.id) ?? estimatedItemHeight), 0
    );
  });

  // Offset for visible items
  let offsetTop = $derived.by(() => {
    const _v = heightVersion;
    return messages.slice(0, visibleRange.start).reduce((sum, m) =>
      sum + (heightMap.get(m.id) ?? estimatedItemHeight), 0
    );
  });

  let visibleMessages = $derived(
    messages.slice(visibleRange.start, visibleRange.end)
  );

  function handleScroll(e: Event) {
    const el = e.target as HTMLDivElement;
    scrollTop = el.scrollTop;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    isUserScrolledUp = distanceFromBottom > 100;
  }

  $effect(() => {
    const _len = messages.length;
    const lastMsg = messages[messages.length - 1];
    const _contentLen = lastMsg?.content?.length ?? 0;
    const _toolCount = lastMsg?.toolCalls?.length ?? 0;
    if (autoScroll && !isUserScrolledUp && container) {
      scheduleAutoScroll();
    }

    return () => {
      if (autoScrollFrame) {
        cancelAnimationFrame(autoScrollFrame);
        autoScrollFrame = 0;
      }
    }
  });

  function measureItem(el: HTMLDivElement, id: string) {
    const observer = new ResizeObserver(([entry]) => {
      const newHeight = Math.round(entry.contentRect.height);
      if (newHeight > 0 && heightMap.get(id) !== newHeight) {
        heightMap.set(id, newHeight);
        // Bump version counter to trigger derived recalculation.
        // This avoids creating a new Map (which would cause a layout
        // change → ResizeObserver → infinite loop).
        scheduleHeightFlush();
      }
    });
    observer.observe(el);
    return {
      destroy() {
        observer.disconnect();
      }
    };
  }

  /** Public: whether user has scrolled away from bottom. */
  export function getIsScrolledUp(): boolean {
    return isUserScrolledUp;
  }

  /** Public: scroll to bottom. */
  export function scrollToBottom(): void {
    isUserScrolledUp = false;
    container?.scrollTo({ top: container.scrollHeight, behavior: 'smooth' });
  }

  $effect(() => {
    return () => {
      if (heightFlushFrame) {
        cancelAnimationFrame(heightFlushFrame);
      }
      if (autoScrollFrame) {
        cancelAnimationFrame(autoScrollFrame);
      }
    };
  });
</script>

<div
  class="virtual-list-container"
  bind:this={container}
  onscroll={handleScroll}
  style="height: {containerHeight}px; overflow-y: auto;"
  role="log"
  aria-live="polite"
  aria-relevant="additions"
>
  <div style="height: {totalHeight}px; position: relative;">
    <div style="transform: translateY({offsetTop}px);">
      {#each visibleMessages as message (message.id)}
        <div use:measureItem={message.id}>
          {@render children({ message })}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .virtual-list-container {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
</style>
