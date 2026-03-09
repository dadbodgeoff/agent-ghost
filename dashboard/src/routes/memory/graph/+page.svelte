<script lang="ts">
  /**
   * Knowledge Graph View (Phase 3, Task 3.6).
   * Force-directed graph of memory entities using d3-force.
   */
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import * as d3 from 'd3-force';
  import { select } from 'd3-selection';
  import { zoom as d3Zoom } from 'd3-zoom';
  import { drag as d3Drag } from 'd3-drag';
  import type { MemoryEntry, MemoryGraphEdge, MemoryGraphNode as ApiMemoryGraphNode } from '@ghost/sdk';

  interface MemoryGraphNode extends ApiMemoryGraphNode, d3.SimulationNodeDatum {}

  let svgEl = $state<SVGSVGElement | null>(null);
  let loading = $state(true);
  let error = $state('');
  let searchQuery = $state('');
  let selectedNode: MemoryGraphNode | null = $state(null);
  let nodeDetail: MemoryEntry | null = $state(null);
  let graphNodes: MemoryGraphNode[] = $state([]);
  let graphEdges: MemoryGraphEdge[] = $state([]);
  let filteredNodeIds: Set<string> | null = $state(null);
  let simulation: ReturnType<typeof d3.forceSimulation<MemoryGraphNode>> | null = null;
  let linkSelection: any = null;
  let nodeSelection: any = null;
  let nodeLabelSelection: any = null;

  const TYPE_COLORS: Record<string, string> = {
    entity: '#7c3aed',
    event: '#0ea5e9',
    concept: '#10b981',
  };

  function nodeRadius(importance: number): number {
    return 6 + importance * 14;
  }

  function nodeOpacity(decay: number): number {
    return 0.3 + (1 - decay) * 0.7;
  }

  function edgeWidth(strength: number): number {
    return 1 + strength * 3;
  }

  async function loadGraph() {
    try {
      const client = await getGhostClient();
      const data = await client.memory.graph();
      graphNodes = data?.nodes ?? [];
      graphEdges = data?.edges ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load knowledge graph';
    }
    loading = false;
  }

  async function loadNodeDetail(nodeId: string) {
    try {
      const client = await getGhostClient();
      nodeDetail = await client.memory.get(nodeId);
    } catch {
      nodeDetail = null;
    }
  }

  function applySearchFilter() {
    if (!nodeSelection || !nodeLabelSelection || !linkSelection) {
      filteredNodeIds = null;
      return;
    }

    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      const matchIds = new Set(
        graphNodes
          .filter((node) => node.label.toLowerCase().includes(q) || node.type.includes(q))
          .map((node) => node.id),
      );
      filteredNodeIds = matchIds;
      nodeSelection.attr('opacity', (node: MemoryGraphNode) => (matchIds.has(node.id) ? 1 : 0.15));
      nodeLabelSelection.attr('opacity', (node: MemoryGraphNode) => (matchIds.has(node.id) ? 1 : 0.1));
      linkSelection.attr('opacity', (edge: MemoryGraphEdge) => {
        const sourceId = typeof edge.source === 'string' ? edge.source : edge.source.id;
        const targetId = typeof edge.target === 'string' ? edge.target : edge.target.id;
        return matchIds.has(sourceId) || matchIds.has(targetId) ? 0.6 : 0.05;
      });
      return;
    }

    filteredNodeIds = null;
    nodeSelection.attr('opacity', (node: MemoryGraphNode) => nodeOpacity(node.decayFactor));
    nodeLabelSelection.attr('opacity', 1);
    linkSelection.attr('opacity', 0.6);
  }

  function renderGraph() {
    if (!svgEl || graphNodes.length === 0) {
      simulation?.stop();
      simulation = null;
      linkSelection = null;
      nodeSelection = null;
      nodeLabelSelection = null;
      filteredNodeIds = null;
      return;
    }

    simulation?.stop();

    const width = svgEl.clientWidth || 800;
    const height = svgEl.clientHeight || 600;

    const svg = select(svgEl);
    svg.selectAll('*').remove();

    const g = svg.append('g');

    // Zoom
    const zoomBehavior = d3Zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => {
        g.attr('transform', event.transform);
      });
    svg.call(zoomBehavior);

    // Simulation
    simulation = d3.forceSimulation<MemoryGraphNode>(graphNodes)
      .force('link', d3.forceLink<MemoryGraphNode, MemoryGraphEdge>(graphEdges)
        .id(d => d.id)
        .distance(80)
        .strength(d => (d as MemoryGraphEdge).strength * 0.5))
      .force('charge', d3.forceManyBody().strength(-120))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide<MemoryGraphNode>().radius(d => nodeRadius(d.importance) + 4));
    const activeSimulation = simulation;

    // Edges
    const link = g.append('g')
      .selectAll('line')
      .data(graphEdges)
      .join('line')
      .attr('stroke', 'var(--color-border-default)')
      .attr('stroke-opacity', 0.6)
      .attr('stroke-width', d => edgeWidth(d.strength));

    // Edge labels
    const linkLabel = g.append('g')
      .selectAll('text')
      .data(graphEdges)
      .join('text')
      .text(d => d.relationship)
      .attr('font-size', '9px')
      .attr('fill', 'var(--color-text-muted)')
      .attr('text-anchor', 'middle')
      .attr('dy', -4);

    // Nodes
    const node = g.append('g')
      .selectAll<SVGCircleElement, MemoryGraphNode>('circle')
      .data(graphNodes)
      .join('circle')
      .attr('r', d => nodeRadius(d.importance))
      .attr('fill', d => TYPE_COLORS[d.type] ?? '#888')
      .attr('fill-opacity', d => nodeOpacity(d.decayFactor))
      .attr('stroke', '#fff')
      .attr('stroke-width', 1.5)
      .attr('cursor', 'pointer')
      .on('click', (_event, d) => {
        selectedNode = d;
        loadNodeDetail(d.id);
      })
      .call(d3Drag<SVGCircleElement, MemoryGraphNode>()
        .on('start', (event, d) => {
          if (!event.active) activeSimulation.alphaTarget(0.3).restart();
          d.fx = d.x;
          d.fy = d.y;
        })
        .on('drag', (event, d) => {
          d.fx = event.x;
          d.fy = event.y;
        })
        .on('end', (event, d) => {
          if (!event.active) activeSimulation.alphaTarget(0);
          d.fx = null;
          d.fy = null;
        })
      );

    // Node labels
    const label = g.append('g')
      .selectAll('text')
      .data(graphNodes)
      .join('text')
      .text(d => d.label)
      .attr('font-size', '10px')
      .attr('fill', 'var(--color-text-primary)')
      .attr('text-anchor', 'middle')
      .attr('dy', d => nodeRadius(d.importance) + 12)
      .attr('pointer-events', 'none');

    activeSimulation.on('tick', () => {
      link
        .attr('x1', d => (d.source as MemoryGraphNode).x!)
        .attr('y1', d => (d.source as MemoryGraphNode).y!)
        .attr('x2', d => (d.target as MemoryGraphNode).x!)
        .attr('y2', d => (d.target as MemoryGraphNode).y!);

      linkLabel
        .attr('x', d => ((d.source as MemoryGraphNode).x! + (d.target as MemoryGraphNode).x!) / 2)
        .attr('y', d => ((d.source as MemoryGraphNode).y! + (d.target as MemoryGraphNode).y!) / 2);

      node
        .attr('cx', d => d.x!)
        .attr('cy', d => d.y!);

      label
        .attr('x', d => d.x!)
        .attr('y', d => d.y!);
    });

    linkSelection = link;
    nodeSelection = node;
    nodeLabelSelection = label;
    applySearchFilter();
  }

  onMount(() => {
    loadGraph();
    return () => {
      simulation?.stop();
    };
  });

  $effect(() => {
    if (!loading && graphNodes.length > 0) {
      // Defer rendering to next tick so SVG is mounted
      requestAnimationFrame(renderGraph);
    }
  });

  $effect(() => {
    searchQuery;
    applySearchFilter();
  });
</script>

<div class="page-header">
  <h1 class="page-title">Knowledge Graph</h1>
  <div class="header-controls">
    <input
      type="text"
      class="search-input"
      bind:value={searchQuery}
      placeholder="Search nodes…"
    />
    <a href="/memory" class="back-link">List View</a>
  </div>
</div>

{#if error}
  <div class="error-banner" role="alert">
    <span>{error}</span>
    <button onclick={() => { error = ''; loadGraph(); }}>Retry</button>
  </div>
{/if}

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else}
  <div class="graph-container">
    <svg bind:this={svgEl} class="graph-svg" width="100%" height="100%"></svg>

    <!-- Legend -->
    <div class="legend">
      <span class="legend-item"><span class="legend-dot" style="background: {TYPE_COLORS.entity}"></span> Entity</span>
      <span class="legend-item"><span class="legend-dot" style="background: {TYPE_COLORS.event}"></span> Event</span>
      <span class="legend-item"><span class="legend-dot" style="background: {TYPE_COLORS.concept}"></span> Concept</span>
      <span class="legend-item legend-count">{graphNodes.length} nodes, {graphEdges.length} edges</span>
    </div>

    <!-- Detail sidebar -->
    {#if selectedNode}
      <div class="detail-sidebar">
        <div class="detail-header">
          <h2>{selectedNode.label}</h2>
          <button class="close-btn" onclick={() => { selectedNode = null; nodeDetail = null; }}>x</button>
        </div>
        <dl class="detail-list">
          <dt>Type</dt><dd class="type-badge" style="color: {TYPE_COLORS[selectedNode.type]}">{selectedNode.type}</dd>
          <dt>Importance</dt><dd>{(selectedNode.importance * 100).toFixed(0)}%</dd>
          <dt>Decay</dt><dd>{(selectedNode.decayFactor * 100).toFixed(0)}%</dd>
          <dt>ID</dt><dd class="mono">{selectedNode.id}</dd>
        </dl>
        {#if nodeDetail}
          <h3>Memory Detail</h3>
          <pre class="detail-json">{JSON.stringify(nodeDetail, null, 2)}</pre>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-4);
  }

  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
  }

  .header-controls {
    display: flex;
    gap: var(--spacing-3);
    align-items: center;
  }

  .search-input {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    width: 200px;
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .graph-container {
    position: relative;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    height: 600px;
    overflow: hidden;
  }

  .graph-svg {
    display: block;
    width: 100%;
    height: 100%;
  }

  .legend {
    position: absolute;
    bottom: var(--spacing-3);
    left: var(--spacing-3);
    display: flex;
    gap: var(--spacing-3);
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
  }

  .legend-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  .legend-count {
    color: var(--color-text-muted);
    margin-left: var(--spacing-2);
  }

  .detail-sidebar {
    position: absolute;
    top: 0;
    right: 0;
    width: 300px;
    height: 100%;
    background: var(--color-bg-elevated-1);
    border-left: 1px solid var(--color-border-default);
    padding: var(--spacing-4);
    overflow-y: auto;
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-3);
  }

  .detail-header h2 {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--color-text-muted);
    font-size: var(--font-size-md);
    cursor: pointer;
  }

  .detail-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--spacing-1) var(--spacing-3);
    margin-bottom: var(--spacing-4);
  }

  .detail-list dt {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .detail-list dd {
    font-size: var(--font-size-sm);
    margin: 0;
  }

  .type-badge {
    font-weight: var(--font-weight-semibold);
    text-transform: capitalize;
  }

  .mono {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    word-break: break-all;
  }

  .detail-sidebar h3 {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-2);
  }

  .detail-json {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 300px;
  }

  .error-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-severity-hard-bg, rgba(255, 0, 0, 0.1));
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
    font-size: var(--font-size-sm);
    color: var(--color-severity-hard);
  }

  .error-banner button {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    text-decoration: underline;
  }

  .skeleton-block {
    height: 600px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }
</style>
