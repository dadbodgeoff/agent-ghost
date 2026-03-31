<script lang="ts">
  /**
   * TraceWaterfall — nested span visualization (waterfall/flame chart).
   * Shows timing bars, token counts, cost per span.
   * Color by span type (agent_run, gate_check, llm_call, tool_exec).
   *
   * Ref: T-3.8.1, DESIGN_SYSTEM §8.4
   */

  interface SpanRecord {
    span_id: string;
    trace_id: string;
    parent_span_id: string | null;
    operation_name: string;
    start_time: string;
    end_time: string | null;
    attributes: Record<string, unknown>;
    status: string;
  }

  interface Props {
    spans?: SpanRecord[];
  }

  let { spans = [] }: Props = $props();

  // Build tree structure from flat spans.
  interface SpanNode extends SpanRecord {
    children: SpanNode[];
    depth: number;
    startMs: number;
    durationMs: number;
    barLeft: number;
    barWidth: number;
  }

  let selectedSpanId = $state<string | null>(null);

  let rootSpans: SpanNode[] = $derived.by(() => {
    if (spans.length === 0) return [];

    const map = new Map<string, SpanNode>();
    let minTime = Infinity;
    let maxTime = 0;

    for (const s of spans) {
      const startMs = new Date(s.start_time).getTime();
      const endMs = s.end_time ? new Date(s.end_time).getTime() : startMs + 1;
      if (startMs < minTime) minTime = startMs;
      if (endMs > maxTime) maxTime = endMs;
      map.set(s.span_id, { ...s, children: [], depth: 0, startMs, durationMs: endMs - startMs, barLeft: 0, barWidth: 0 });
    }

    const totalDuration = Math.max(maxTime - minTime, 1);
    const roots: SpanNode[] = [];

    for (const node of map.values()) {
      node.barLeft = ((node.startMs - minTime) / totalDuration) * 100;
      node.barWidth = Math.max((node.durationMs / totalDuration) * 100, 0.5);
      if (node.parent_span_id && map.has(node.parent_span_id)) {
        const parent = map.get(node.parent_span_id)!;
        node.depth = parent.depth + 1;
        parent.children.push(node);
      } else {
        roots.push(node);
      }
    }

    return roots;
  });

  function flattenTree(nodes: SpanNode[]): SpanNode[] {
    const result: SpanNode[] = [];
    function walk(n: SpanNode) {
      result.push(n);
      for (const child of n.children.sort((a, b) => a.startMs - b.startMs)) {
        walk(child);
      }
    }
    for (const root of nodes) walk(root);
    return result;
  }

  let flatSpans: SpanNode[] = $derived(flattenTree(rootSpans));

  const TYPE_COLORS: Record<string, string> = {
    agent_run: 'var(--color-chart-1)',
    agent_pre_loop: 'var(--color-chart-1)',
    gate_check: 'var(--color-chart-5)',
    check_gates: 'var(--color-chart-5)',
    llm_call: 'var(--color-chart-2)',
    tool_exec: 'var(--color-chart-3)',
    scan: 'var(--color-chart-4)',
    extract: 'var(--color-chart-6)',
    convergence_watcher_poll: 'var(--color-chart-7)',
  };

  function spanColor(op: string): string {
    for (const [key, color] of Object.entries(TYPE_COLORS)) {
      if (op.includes(key)) return color;
    }
    return 'var(--color-brand-primary)';
  }

  function formatDuration(ms: number): string {
    if (ms < 1) return '<1ms';
    if (ms < 1000) return `${Math.round(ms)}ms`;
    return `${(ms / 1000).toFixed(2)}s`;
  }

  function getTokens(attrs: Record<string, unknown>): string {
    const input = attrs['gen_ai.usage.input_tokens'] ?? attrs['input_tokens'];
    const output = attrs['gen_ai.usage.output_tokens'] ?? attrs['output_tokens'];
    if (input != null || output != null) {
      return `${input ?? '?'}/${output ?? '?'}`;
    }
    return '';
  }

  function getCost(attrs: Record<string, unknown>): string {
    const cost = attrs['gen_ai.usage.cost'] ?? attrs['cost'];
    if (cost != null) return `$${Number(cost).toFixed(4)}`;
    return '';
  }

  let expandedIds: Set<string> = $state(new Set());

  function toggleExpand(id: string) {
    const next = new Set(expandedIds);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expandedIds = next;
  }
</script>

<div class="trace-waterfall" role="tree" aria-label={`Trace waterfall with ${spans.length} spans`}>
  {#if spans.length === 0}
    <p class="empty">No trace data. Select a session to view spans.</p>
  {:else}
    <div class="header-row">
      <span class="col-name">Operation</span>
      <span class="col-timeline">Timeline</span>
      <span class="col-duration">Duration</span>
      <span class="col-tokens">Tokens</span>
      <span class="col-cost">Cost</span>
      <span class="col-status">Status</span>
    </div>
    {#each flatSpans as span (span.span_id)}
      <div
        class="span-row"
        class:expanded={expandedIds.has(span.span_id)}
        role="treeitem"
        aria-selected={selectedSpanId === span.span_id}
        aria-expanded={span.children.length > 0 ? expandedIds.has(span.span_id) : undefined}
        aria-level={span.depth + 1}
        style="padding-left: {span.depth * 20 + 8}px"
      >
        <span class="col-name">
          {#if span.children.length > 0}
            <button class="toggle" onclick={() => toggleExpand(span.span_id)}>
              {expandedIds.has(span.span_id) ? '▼' : '▶'}
            </button>
          {:else}
            <span class="toggle-spacer"></span>
          {/if}
          <span class="op-name" title={span.operation_name}>{span.operation_name}</span>
        </span>
        <span class="col-timeline">
          <div class="bar-track">
            <div
              class="bar-fill"
              style="left: {span.barLeft}%; width: {span.barWidth}%; background: {spanColor(span.operation_name)};"
            ></div>
          </div>
        </span>
        <span class="col-duration mono">{formatDuration(span.durationMs)}</span>
        <span class="col-tokens mono">{getTokens(span.attributes)}</span>
        <span class="col-cost mono">{getCost(span.attributes)}</span>
        <span class="col-status">
          <span class="status-dot" class:ok={span.status === 'ok'} class:error={span.status === 'error'}></span>
        </span>
      </div>
    {/each}
  {/if}
</div>

<style>
  .trace-waterfall {
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    overflow: hidden;
    font-size: var(--font-size-sm);
  }

  .header-row {
    display: flex;
    align-items: center;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border-bottom: 1px solid var(--color-border-default);
    font-weight: 600;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .span-row {
    display: flex;
    align-items: center;
    padding: var(--spacing-1) var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
    transition: background 0.1s;
  }

  .span-row:hover {
    background: var(--color-bg-elevated-1);
  }

  .col-name {
    flex: 0 0 200px;
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
    overflow: hidden;
    white-space: nowrap;
  }

  .col-timeline {
    flex: 1;
    padding: 0 var(--spacing-2);
  }

  .col-duration {
    flex: 0 0 80px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-secondary);
  }

  .col-tokens {
    flex: 0 0 80px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .col-cost {
    flex: 0 0 60px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .col-status {
    flex: 0 0 40px;
    text-align: center;
  }

  .toggle {
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    width: 16px;
    line-height: 1;
  }

  .toggle-spacer {
    display: inline-block;
    width: 16px;
  }

  .op-name {
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--color-text-primary);
  }

  .bar-track {
    position: relative;
    height: 12px;
    background: var(--color-bg-elevated-2);
    border-radius: 2px;
  }

  .bar-fill {
    position: absolute;
    top: 0;
    height: 100%;
    border-radius: 2px;
    min-width: 2px;
    opacity: 0.85;
  }

  .status-dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-text-muted);
  }

  .status-dot.ok {
    background: var(--color-severity-normal);
  }

  .status-dot.error {
    background: var(--color-severity-hard);
  }

  .empty {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .mono {
    font-family: var(--font-family-mono);
  }
</style>
