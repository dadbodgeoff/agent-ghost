<script lang="ts">
  /**
   * WorkflowCanvas — visual DAG editor for the currently supported runtime nodes.
   * The canvas is intentionally limited to semantics the backend can execute.
   */

  export interface WorkflowNode {
    id: string;
    type: 'llm_call' | 'tool_exec' | 'gate_check' | 'transform' | 'condition' | 'wait' | string;
    label: string;
    x: number;
    y: number;
    config: Record<string, unknown>;
    branch_group?: string;
    condition?: string;
    execution_status?: 'pending' | 'running' | 'completed' | 'failed' | 'skipped' | 'passed';
  }

  export interface WorkflowEdge {
    source: string;
    target: string;
    condition_label?: string;
    branch_type?: 'parallel' | 'conditional' | 'default';
  }

  let {
    nodes = $bindable([]),
    edges = $bindable([]),
    selectedNodeId = $bindable(null),
    onnodeselect,
    onnodedblclick,
  }: {
    nodes: WorkflowNode[];
    edges: WorkflowEdge[];
    selectedNodeId: string | null;
    onnodeselect?: (nodeId: string) => void;
    onnodedblclick?: (nodeId: string) => void;
  } = $props();

  let svgEl: SVGSVGElement;
  let dragging: string | null = $state(null);
  let dragOffset = { x: 0, y: 0 };
  let viewBox = $state({ x: -50, y: -50, w: 800, h: 600 });
  let panning = $state(false);
  let panStart = { x: 0, y: 0, vx: 0, vy: 0 };

  // Touch gesture state
  let pinchDistance: number | null = null;
  let pinchViewBox: typeof viewBox | null = null;

  // Connecting mode
  let connecting: string | null = $state(null);
  let connectTarget = $state({ x: 0, y: 0 });

  const NODE_W = 140;
  const NODE_H = 50;
  const DIAMOND_SIZE = 40;

  const NODE_COLORS: Record<string, string> = {
    llm_call: 'var(--color-severity-soft)',
    tool_exec: 'var(--color-severity-active)',
    gate_check: 'var(--color-severity-normal)',
    transform: 'var(--color-brand-primary)',
    condition: 'var(--color-severity-hard)',
    wait: 'var(--color-score-mid)',
  };

  const EXEC_STATUS_COLORS: Record<string, string> = {
    pending: 'var(--color-text-muted)',
    running: 'var(--color-severity-active)',
    completed: 'var(--color-score-high)',
    failed: 'var(--color-severity-hard)',
    skipped: 'var(--color-text-muted)',
    passed: 'var(--color-score-high)',
  };

  function nodeColor(type: string): string {
    return NODE_COLORS[type] ?? 'var(--color-text-muted)';
  }

  function getNode(id: string): WorkflowNode | undefined {
    return nodes.find(n => n.id === id);
  }

  function isConditionNode(type: string): boolean {
    return type === 'condition';
  }

  function isSubWorkflow(type: string): boolean {
    return false;
  }

  // ── Pointer handlers ─────────────────────────────────────────────

  function handleNodePointerDown(e: PointerEvent, nodeId: string) {
    e.stopPropagation();
    if (e.shiftKey) {
      connecting = nodeId;
      const node = getNode(nodeId);
      if (node) {
        connectTarget = { x: node.x + NODE_W, y: node.y + NODE_H / 2 };
      }
      (e.target as Element).setPointerCapture(e.pointerId);
      return;
    }
    dragging = nodeId;
    const node = getNode(nodeId)!;
    const pt = svgPoint(e);
    dragOffset = { x: pt.x - node.x, y: pt.y - node.y };
    (e.target as Element).setPointerCapture(e.pointerId);
    selectedNodeId = nodeId;
    onnodeselect?.(nodeId);
  }

  function handleNodeDblClick(nodeId: string) {
    onnodedblclick?.(nodeId);
  }

  function handlePointerMove(e: PointerEvent) {
    if (dragging) {
      const pt = svgPoint(e);
      nodes = nodes.map(n =>
        n.id === dragging ? { ...n, x: pt.x - dragOffset.x, y: pt.y - dragOffset.y } : n
      );
    } else if (connecting) {
      const pt = svgPoint(e);
      connectTarget = { x: pt.x, y: pt.y };
    } else if (panning) {
      const dx = (e.clientX - panStart.x) * (viewBox.w / svgEl.clientWidth);
      const dy = (e.clientY - panStart.y) * (viewBox.h / svgEl.clientHeight);
      viewBox = { ...viewBox, x: panStart.vx - dx, y: panStart.vy - dy };
    }
  }

  function handlePointerUp(e: PointerEvent) {
    if (connecting) {
      const pt = svgPoint(e);
      const targetNode = nodes.find(n =>
        pt.x >= n.x && pt.x <= n.x + NODE_W && pt.y >= n.y && pt.y <= n.y + NODE_H
      );
      if (targetNode && targetNode.id !== connecting) {
        const exists = edges.some(e => e.source === connecting && e.target === targetNode.id);
        if (!exists) {
          edges = [...edges, { source: connecting!, target: targetNode.id }];
        }
      }
      connecting = null;
    }
    dragging = null;
    panning = false;
  }

  function handleCanvasPointerDown(e: PointerEvent) {
    if (e.target === svgEl || (e.target as Element).classList.contains('canvas-bg')) {
      panning = true;
      panStart = { x: e.clientX, y: e.clientY, vx: viewBox.x, vy: viewBox.y };
      selectedNodeId = null;
    }
  }

  function handleWheel(e: WheelEvent) {
    e.preventDefault();
    const scale = e.deltaY > 0 ? 1.1 : 0.9;
    const cx = viewBox.x + viewBox.w / 2;
    const cy = viewBox.y + viewBox.h / 2;
    const nw = viewBox.w * scale;
    const nh = viewBox.h * scale;
    viewBox = { x: cx - nw / 2, y: cy - nh / 2, w: nw, h: nh };
  }

  // ── Touch gesture handlers (T-4.10.2) ─────────────────────────

  function handleTouchStart(e: TouchEvent) {
    if (e.touches.length === 2) {
      e.preventDefault();
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      pinchDistance = Math.sqrt(dx * dx + dy * dy);
      pinchViewBox = { ...viewBox };
    }
  }

  function handleTouchMove(e: TouchEvent) {
    if (e.touches.length === 2 && pinchDistance && pinchViewBox) {
      e.preventDefault();
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      const newDist = Math.sqrt(dx * dx + dy * dy);
      const scale = pinchDistance / newDist;
      const cx = pinchViewBox.x + pinchViewBox.w / 2;
      const cy = pinchViewBox.y + pinchViewBox.h / 2;
      const nw = pinchViewBox.w * scale;
      const nh = pinchViewBox.h * scale;
      viewBox = { x: cx - nw / 2, y: cy - nh / 2, w: nw, h: nh };
    }
  }

  function handleTouchEnd() {
    pinchDistance = null;
    pinchViewBox = null;
  }

  function svgPoint(e: PointerEvent): { x: number; y: number } {
    if (!svgEl) return { x: 0, y: 0 };
    const rect = svgEl.getBoundingClientRect();
    return {
      x: viewBox.x + ((e.clientX - rect.left) / rect.width) * viewBox.w,
      y: viewBox.y + ((e.clientY - rect.top) / rect.height) * viewBox.h,
    };
  }

  // ── Edge rendering ────────────────────────────────────────────

  function edgePath(edge: WorkflowEdge): string {
    const src = getNode(edge.source);
    const tgt = getNode(edge.target);
    if (!src || !tgt) return '';
    const x1 = src.x + NODE_W;
    const y1 = src.y + NODE_H / 2;
    const x2 = tgt.x;
    const y2 = tgt.y + NODE_H / 2;
    const mx = (x1 + x2) / 2;
    return `M ${x1} ${y1} C ${mx} ${y1}, ${mx} ${y2}, ${x2} ${y2}`;
  }

  function edgeStroke(edge: WorkflowEdge): string {
    if (edge.branch_type === 'parallel') return 'var(--color-score-mid)';
    if (edge.branch_type === 'conditional') return 'var(--color-severity-hard)';
    // Glow for active execution path.
    const src = getNode(edge.source);
    if (src?.execution_status === 'running') return 'var(--color-severity-active)';
    if (src?.execution_status === 'completed') return 'var(--color-score-high)';
    return 'var(--color-text-muted)';
  }

  function edgeDashArray(edge: WorkflowEdge): string {
    if (edge.branch_type === 'parallel') return '6 3';
    return 'none';
  }

  // ── Branch group rendering ────────────────────────────────────

  let branchGroups = $derived.by(() => {
    const groups = new Map<string, { minX: number; minY: number; maxX: number; maxY: number }>();
    for (const node of nodes) {
      if (!node.branch_group) continue;
      const existing = groups.get(node.branch_group);
      if (existing) {
        existing.minX = Math.min(existing.minX, node.x);
        existing.minY = Math.min(existing.minY, node.y);
        existing.maxX = Math.max(existing.maxX, node.x + NODE_W);
        existing.maxY = Math.max(existing.maxY, node.y + NODE_H);
      } else {
        groups.set(node.branch_group, {
          minX: node.x,
          minY: node.y,
          maxX: node.x + NODE_W,
          maxY: node.y + NODE_H,
        });
      }
    }
    return Array.from(groups.entries());
  });

  // ── Public API ─────────────────────────────────────────────────

  export function addNode(type: string, label: string) {
    const id = crypto.randomUUID();
    const x = viewBox.x + viewBox.w / 2 - NODE_W / 2;
    const y = viewBox.y + viewBox.h / 2 - NODE_H / 2;
    nodes = [...nodes, { id, type, label, x, y, config: {} }];
    selectedNodeId = id;
    onnodeselect?.(id);
  }

  export function removeNode(nodeId: string) {
    nodes = nodes.filter(n => n.id !== nodeId);
    edges = edges.filter(e => e.source !== nodeId && e.target !== nodeId);
    if (selectedNodeId === nodeId) selectedNodeId = null;
  }

  export function removeEdge(source: string, target: string) {
    edges = edges.filter(e => !(e.source === source && e.target === target));
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  bind:this={svgEl}
  class="workflow-canvas"
  viewBox="{viewBox.x} {viewBox.y} {viewBox.w} {viewBox.h}"
  onpointerdown={handleCanvasPointerDown}
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
  onwheel={handleWheel}
  ontouchstart={handleTouchStart}
  ontouchmove={handleTouchMove}
  ontouchend={handleTouchEnd}
  role="img"
  aria-label="Workflow canvas"
>
  <defs>
    <marker id="wf-arrow" viewBox="0 0 10 6" refX="10" refY="3" markerWidth="8" markerHeight="6" orient="auto-start-reverse">
      <path d="M 0 0 L 10 3 L 0 6 z" fill="var(--color-text-muted)" />
    </marker>
    <filter id="glow-running">
      <feGaussianBlur stdDeviation="3" result="blur" />
      <feMerge>
        <feMergeNode in="blur" />
        <feMergeNode in="SourceGraphic" />
      </feMerge>
    </filter>
  </defs>

  <rect class="canvas-bg" x={viewBox.x} y={viewBox.y} width={viewBox.w} height={viewBox.h} fill="transparent" />

  <!-- Branch group dashed rectangles -->
  {#each branchGroups as [groupId, bounds]}
    <rect
      x={bounds.minX - 15}
      y={bounds.minY - 15}
      width={bounds.maxX - bounds.minX + 30}
      height={bounds.maxY - bounds.minY + 30}
      rx="8"
      fill="none"
      stroke="var(--color-score-mid)"
      stroke-width="1"
      stroke-dasharray="6 3"
      opacity="0.4"
    />
    <text
      x={bounds.minX - 10}
      y={bounds.minY - 20}
      font-size="9"
      fill="var(--color-score-mid)"
      opacity="0.6"
    >{groupId}</text>
  {/each}

  <!-- Edges -->
  {#each edges as edge}
    <path
      d={edgePath(edge)}
      fill="none"
      stroke={edgeStroke(edge)}
      stroke-width={edge.branch_type === 'parallel' ? 2 : 1.5}
      stroke-dasharray={edgeDashArray(edge)}
      marker-end="url(#wf-arrow)"
      opacity="0.7"
      class:active-edge={getNode(edge.source)?.execution_status === 'running'}
    />
    {#if edge.condition_label}
      {@const src = getNode(edge.source)}
      {@const tgt = getNode(edge.target)}
      {#if src && tgt}
        <text
          x={(src.x + NODE_W + tgt.x) / 2}
          y={(src.y + tgt.y) / 2 + NODE_H / 2 - 5}
          font-size="9"
          fill="var(--color-severity-hard)"
          text-anchor="middle"
        >{edge.condition_label}</text>
      {/if}
    {/if}
  {/each}

  <!-- Connecting line -->
  {#if connecting}
    {@const src = getNode(connecting)}
    {#if src}
      <line
        x1={src.x + NODE_W} y1={src.y + NODE_H / 2}
        x2={connectTarget.x} y2={connectTarget.y}
        stroke="var(--color-interactive-primary)" stroke-width="2" stroke-dasharray="4 2"
      />
    {/if}
  {/if}

  <!-- Nodes -->
  {#each nodes as node (node.id)}
    <g
      class="wf-node"
      class:selected={selectedNodeId === node.id}
      class:running={node.execution_status === 'running'}
      onpointerdown={(e) => handleNodePointerDown(e, node.id)}
      ondblclick={() => handleNodeDblClick(node.id)}
      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); selectedNodeId = node.id; onnodeselect?.(node.id); } if (e.key === 'Delete' || e.key === 'Backspace') { removeNode(node.id); } }}
      role="button"
      tabindex="0"
      aria-label="{node.type}: {node.label}"
    >
      {#if isConditionNode(node.type)}
        <!-- Diamond shape for condition nodes -->
        <polygon
          points="{node.x + NODE_W/2},{node.y} {node.x + NODE_W},{node.y + NODE_H/2} {node.x + NODE_W/2},{node.y + NODE_H} {node.x},{node.y + NODE_H/2}"
          fill="var(--color-bg-elevated-1)"
          stroke={selectedNodeId === node.id ? 'var(--color-interactive-primary)' : nodeColor(node.type)}
          stroke-width={selectedNodeId === node.id ? 2 : 1.5}
        />
      {:else}
        <rect
          x={node.x} y={node.y}
          width={NODE_W} height={NODE_H}
          rx="6" ry="6"
          fill="var(--color-bg-elevated-1)"
          stroke={selectedNodeId === node.id ? 'var(--color-interactive-primary)' : nodeColor(node.type)}
          stroke-width={selectedNodeId === node.id ? 2 : 1.5}
          class:double-border={isSubWorkflow(node.type)}
        />
        {#if isSubWorkflow(node.type)}
          <!-- Inner border for sub-workflow -->
          <rect
            x={node.x + 3} y={node.y + 3}
            width={NODE_W - 6} height={NODE_H - 6}
            rx="4" ry="4"
            fill="none"
            stroke={nodeColor(node.type)}
            stroke-width="0.5"
            opacity="0.5"
          />
        {/if}
      {/if}

      <!-- Type indicator dot -->
      <circle cx={node.x + 12} cy={node.y + NODE_H / 2} r="4" fill={nodeColor(node.type)} />

      <!-- Label -->
      <text x={node.x + 22} y={node.y + NODE_H / 2 + 4} font-size="11" fill="var(--color-text-primary)">
        {node.label.length > 14 ? node.label.slice(0, 14) + '...' : node.label}
      </text>

      <!-- Execution status overlay -->
      {#if node.execution_status}
        {@const statusColor = EXEC_STATUS_COLORS[node.execution_status] ?? 'var(--color-text-muted)'}
        {#if node.execution_status === 'running'}
          <circle
            cx={node.x + NODE_W - 12}
            cy={node.y + 12}
            r="5"
            fill={statusColor}
            class="pulse"
          />
        {:else if node.execution_status === 'completed'}
          <text
            x={node.x + NODE_W - 16}
            y={node.y + 16}
            font-size="12"
            fill={statusColor}
          >&#10003;</text>
        {:else if node.execution_status === 'failed'}
          <text
            x={node.x + NODE_W - 16}
            y={node.y + 16}
            font-size="12"
            fill={statusColor}
          >&#10007;</text>
        {/if}
      {/if}

      <!-- A/B test variant labels -->
    </g>
  {/each}
</svg>

<style>
  .workflow-canvas {
    width: 100%;
    height: 500px;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    cursor: grab;
    touch-action: none;
  }

  .workflow-canvas:active {
    cursor: grabbing;
  }

  .wf-node {
    cursor: move;
  }

  .wf-node:hover rect,
  .wf-node:hover polygon {
    filter: brightness(1.1);
  }

  .wf-node.running rect,
  .wf-node.running polygon {
    filter: url(#glow-running);
  }

  .active-edge {
    filter: url(#glow-running);
    stroke-width: 2.5;
  }

  .pulse {
    animation: pulse-anim 1.2s ease-in-out infinite;
  }

  @keyframes pulse-anim {
    0%, 100% { opacity: 1; r: 5; }
    50% { opacity: 0.4; r: 7; }
  }
</style>
