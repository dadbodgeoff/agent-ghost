<script lang="ts">
  /**
   * CausalGraph — DAG visualization of agent actions using d3-force.
   * d3 computes layout, Svelte renders SVG (no d3 DOM manipulation).
   * Cap at 500 nodes, zoom/pan via SVG viewBox.
   *
   * Ref: T-2.2.1, ADE_DESIGN_PLAN §5.2.1, DESIGN_SYSTEM §8.4
   */
  import { onMount } from 'svelte';
  import {
    forceSimulation,
    forceLink,
    forceManyBody,
    forceCenter,
    forceCollide,
    type SimulationNodeDatum,
    type SimulationLinkDatum,
  } from 'd3-force';

  interface GraphNode {
    id: string;
    label: string;
    type: string;
  }

  interface GraphEdge {
    from: string;
    to: string;
    label?: string;
  }

  interface Props {
    nodes?: GraphNode[];
    edges?: GraphEdge[];
    selectedNodeId?: string | null;
    onnodeclick?: (node: GraphNode) => void;
  }

  let {
    nodes = [],
    edges = [],
    selectedNodeId = null,
    onnodeclick,
  }: Props = $props();

  const MAX_NODES = 500;

  const TYPE_COLORS: Record<string, string> = {
    llm_call: 'var(--color-chart-1)',
    tool_exec: 'var(--color-chart-2)',
    proposal: 'var(--color-chart-3)',
    gate_check: 'var(--color-chart-5)',
    intervention: 'var(--color-severity-hard)',
  };

  function nodeColor(type: string): string {
    return TYPE_COLORS[type] || 'var(--color-brand-primary)';
  }

  // Simulation state
  interface SimNode extends SimulationNodeDatum {
    id: string;
    label: string;
    type: string;
  }

  interface SimLink extends SimulationLinkDatum<SimNode> {
    label?: string;
  }

  let simNodes: SimNode[] = $state([]);
  let simLinks: SimLink[] = $state([]);
  let ticked = $state(0); // Force reactivity on tick

  // Zoom/pan state
  let viewBox = $state({ x: -350, y: -250, w: 700, h: 500 });
  let isPanning = $state(false);
  let panStart = $state({ x: 0, y: 0 });

  function initSimulation() {
    const cappedNodes = nodes.slice(0, MAX_NODES);
    const nodeIds = new Set(cappedNodes.map(n => n.id));

    const sNodes: SimNode[] = cappedNodes.map(n => ({
      id: n.id,
      label: n.label,
      type: n.type,
      x: undefined,
      y: undefined,
    }));

    const sLinks: SimLink[] = edges
      .filter(e => nodeIds.has(e.from) && nodeIds.has(e.to))
      .map(e => ({
        source: e.from,
        target: e.to,
        label: e.label,
      }));

    const sim = forceSimulation(sNodes)
      .force('link', forceLink<SimNode, SimLink>(sLinks).id(d => d.id).distance(80))
      .force('charge', forceManyBody().strength(-150))
      .force('center', forceCenter(0, 0))
      .force('collide', forceCollide(30))
      .on('tick', () => {
        simNodes = sNodes;
        simLinks = sLinks;
        ticked++;
      });

    // Let simulation warm up.
    sim.alpha(1).restart();
  }

  $effect(() => {
    if (nodes.length > 0) {
      initSimulation();
    }
  });

  function handleWheel(e: WheelEvent) {
    e.preventDefault();
    const scale = e.deltaY > 0 ? 1.1 : 0.9;
    viewBox = {
      x: viewBox.x,
      y: viewBox.y,
      w: viewBox.w * scale,
      h: viewBox.h * scale,
    };
  }

  function handlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    isPanning = true;
    panStart = { x: e.clientX, y: e.clientY };
  }

  function handlePointerMove(e: PointerEvent) {
    if (!isPanning) return;
    const dx = (e.clientX - panStart.x) * (viewBox.w / 700);
    const dy = (e.clientY - panStart.y) * (viewBox.h / 500);
    viewBox = {
      ...viewBox,
      x: viewBox.x - dx,
      y: viewBox.y - dy,
    };
    panStart = { x: e.clientX, y: e.clientY };
  }

  function handlePointerUp() {
    isPanning = false;
  }

  function linkSource(link: SimLink): SimNode | undefined {
    return typeof link.source === 'object' ? (link.source as SimNode) : undefined;
  }

  function linkTarget(link: SimLink): SimNode | undefined {
    return typeof link.target === 'object' ? (link.target as SimNode) : undefined;
  }
</script>

<div class="causal-graph" role="img" aria-label="Causal relationship graph with {nodes.length} nodes and {edges.length} edges">
  {#if nodes.length === 0}
    <p class="empty">No graph data available.</p>
  {:else}
    {#if nodes.length > MAX_NODES}
      <p class="cap-notice">Showing {MAX_NODES} of {nodes.length} nodes</p>
    {/if}
    <svg
      viewBox="{viewBox.x} {viewBox.y} {viewBox.w} {viewBox.h}"
      class="graph-svg"
      onwheel={handleWheel}
      onpointerdown={handlePointerDown}
      onpointermove={handlePointerMove}
      onpointerup={handlePointerUp}
    >
      <defs>
        <marker id="arrowhead" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
          <polygon points="0 0, 10 3.5, 0 7" fill="var(--color-border-primary)" />
        </marker>
      </defs>

      <!-- Edges -->
      {#each simLinks as link}
        {@const s = linkSource(link)}
        {@const t = linkTarget(link)}
        {#if s && t && s.x != null && t.x != null}
          <line
            x1={s.x} y1={s.y}
            x2={t.x} y2={t.y}
            stroke="var(--color-border-primary)"
            stroke-width="1.5"
            marker-end="url(#arrowhead)"
            opacity="0.6"
          />
        {/if}
      {/each}

      <!-- Nodes -->
      {#each simNodes as node}
        {#if node.x != null && node.y != null}
          <g
            class="node-group"
            transform="translate({node.x}, {node.y})"
            onclick={() => onnodeclick?.(node)}
            role="button"
            tabindex="0"
            onkeydown={(e) => { if (e.key === 'Enter') onnodeclick?.(node); }}
          >
            <circle
              r="20"
              fill="var(--color-bg-secondary)"
              stroke={nodeColor(node.type)}
              stroke-width={selectedNodeId === node.id ? 3 : 1.5}
              class:selected={selectedNodeId === node.id}
            />
            <text
              y="4"
              text-anchor="middle"
              fill="var(--color-text-primary)"
              font-size="9"
              font-family="var(--font-family-mono)"
            >
              {node.label.length > 8 ? node.label.slice(0, 7) + '…' : node.label}
            </text>
          </g>
        {/if}
      {/each}
    </svg>
  {/if}
</div>

<style>
  .causal-graph {
    background: var(--color-bg-primary);
    border: 1px solid var(--color-border-primary);
    border-radius: var(--radius-md);
    padding: var(--spacing-2);
    overflow: hidden;
    position: relative;
  }

  .graph-svg {
    width: 100%;
    height: 400px;
    cursor: grab;
    touch-action: none;
  }

  .graph-svg:active {
    cursor: grabbing;
  }

  .node-group {
    cursor: pointer;
    outline: none;
  }

  .node-group:focus-visible circle {
    stroke-width: 3;
    filter: drop-shadow(0 0 4px var(--color-interactive-primary));
  }

  .node-group:hover circle {
    filter: brightness(1.1);
  }

  circle.selected {
    filter: drop-shadow(0 0 6px var(--color-interactive-primary));
  }

  .empty {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-tertiary);
    font-size: var(--font-size-sm);
  }

  .cap-notice {
    font-size: var(--font-size-xs);
    color: var(--color-severity-soft);
    text-align: center;
    margin-bottom: var(--spacing-1);
  }
</style>
