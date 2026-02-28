<script lang="ts">
  export let nodes: { id: string; label: string; type: string }[] = [];
  export let edges: { from: string; to: string; label?: string }[] = [];

  // Simple force-directed layout placeholder.
  // In production, use d3-force or a dedicated graph library.
  $: nodeMap = new Map(nodes.map((n, i) => [n.id, {
    ...n,
    x: 100 + (i % 4) * 150,
    y: 80 + Math.floor(i / 4) * 120,
  }]));
</script>

<div class="causal-graph" role="img" aria-label="Causal relationship graph">
  <svg viewBox="0 0 700 400" class="graph-svg">
    {#each edges as edge}
      {@const from = nodeMap.get(edge.from)}
      {@const to = nodeMap.get(edge.to)}
      {#if from && to}
        <line x1={from.x} y1={from.y} x2={to.x} y2={to.y} stroke="#3f3f46" stroke-width="1.5" />
      {/if}
    {/each}
    {#each [...nodeMap.values()] as node}
      <circle cx={node.x} cy={node.y} r="24" fill="#1a1a2e" stroke="#a0a0ff" stroke-width="1.5" />
      <text x={node.x} y={node.y + 4} text-anchor="middle" fill="#e4e4e7" font-size="10">{node.label}</text>
    {/each}
  </svg>
</div>

<style>
  .causal-graph { background: #0d0d1a; border: 1px solid #27272a; border-radius: 8px; padding: 8px; }
  .graph-svg { width: 100%; height: auto; }
</style>
