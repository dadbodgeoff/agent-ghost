<!--
  HashChainStrip — Horizontal hash chain visualization (T-X.13).

  Displays a horizontal strip of linked hash blocks. Breaks in the
  chain are highlighted in red. Each block shows the first 8 chars
  of the hash with a connecting arrow to the next block.

  Ref: ADE_DESIGN_PLAN §5.3.3, tasks.md T-X.13
-->
<script lang="ts">
  interface ChainBlock {
    event_hash: string;
    previous_hash: string;
    event_id?: string;
    position?: number;
  }

  interface ChainBreak {
    position: number;
    event_id?: string;
  }

  interface Props {
    blocks?: ChainBlock[];
    breaks?: ChainBreak[];
    maxVisible?: number;
    onblockclick?: (block: ChainBlock, index: number) => void;
  }

  let { blocks = [], breaks = [], maxVisible = 20, onblockclick }: Props = $props();

  let breakPositions = $derived(new Set(breaks.map(b => b.position)));

  let visibleBlocks = $derived(
    blocks.length > maxVisible
      ? blocks.slice(0, maxVisible)
      : blocks
  );

  let hasMore = $derived(blocks.length > maxVisible);

  function shortHash(hash: string): string {
    if (!hash) return '--------';
    return hash.slice(0, 8);
  }

  function isBreak(index: number): boolean {
    return breakPositions.has(index);
  }
</script>

<div class="hash-chain-strip" role="group" aria-label="Hash chain">
  {#each visibleBlocks as block, i}
    {#if i > 0}
      <div
        class="chain-link"
        class:broken={isBreak(i)}
        aria-hidden="true"
      >
        {isBreak(i) ? '\u2717' : '\u2192'}
      </div>
    {/if}
    <button
      class="chain-block"
      class:genesis={i === 0}
      class:broken={isBreak(i)}
      type="button"
      title={`Hash: ${block.event_hash}\nPrev: ${block.previous_hash}`}
      onclick={() => onblockclick?.(block, i)}
    >
      <span class="block-hash">{shortHash(block.event_hash)}</span>
      {#if block.position !== undefined}
        <span class="block-seq">#{block.position}</span>
      {/if}
    </button>
  {/each}

  {#if hasMore}
    <div class="chain-more">
      +{blocks.length - maxVisible} more
    </div>
  {/if}
</div>

<style>
  .hash-chain-strip {
    display: flex;
    align-items: center;
    gap: 0;
    overflow-x: auto;
    padding: var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
    scrollbar-width: thin;
  }

  .chain-block {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--spacing-0-5);
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all var(--duration-fast) var(--easing-default);
    flex-shrink: 0;
    font-family: var(--font-family-mono);
  }

  .chain-block:hover {
    background: var(--color-surface-hover);
    border-color: var(--color-interactive-primary);
  }

  .chain-block:focus-visible {
    box-shadow: var(--shadow-focus-ring);
    outline: none;
  }

  .chain-block.genesis {
    border-color: var(--color-severity-normal);
    background: color-mix(in srgb, var(--color-severity-normal) 10%, var(--color-bg-elevated-2));
  }

  .chain-block.broken {
    border-color: var(--color-severity-hard);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, var(--color-bg-elevated-2));
  }

  .block-hash {
    font-size: var(--font-size-xs);
    color: var(--color-text-primary);
    font-weight: var(--font-weight-medium);
    letter-spacing: var(--letter-spacing-tight);
  }

  .block-seq {
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
  }

  .chain-link {
    flex-shrink: 0;
    padding: 0 var(--spacing-1);
    font-size: var(--font-size-sm);
    color: var(--color-text-disabled);
  }

  .chain-link.broken {
    color: var(--color-severity-hard);
    font-weight: var(--font-weight-bold);
  }

  .chain-more {
    flex-shrink: 0;
    padding: var(--spacing-1) var(--spacing-2);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-style: italic;
  }
</style>
