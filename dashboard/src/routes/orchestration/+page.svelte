<script lang="ts">
  /**
   * Orchestration page — trust graph, consensus state, sybil resistance, A2A discovery.
   * Reuses d3-force pattern from CausalGraph.
   *
   * Ref: T-3.9.1, T-3.9.2, T-3.9.3, T-4.4.1
   */
  import { onMount } from 'svelte';
  import type {
    A2ATask,
    ConsensusRound,
    Delegation,
    DiscoveredA2AAgent,
    SybilMetrics,
    TrustEdge,
    TrustNode,
  } from '@ghost/sdk';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import A2AAgentCard from '../../components/A2AAgentCard.svelte';
  import A2ATaskTracker from '../../components/A2ATaskTracker.svelte';
  import {
    forceSimulation,
    forceLink,
    forceManyBody,
    forceCenter,
    forceCollide,
    type SimulationNodeDatum,
    type SimulationLinkDatum,
  } from 'd3-force';

  // ── Types ────────────────────────────────────────────────────────

  // ── State ────────────────────────────────────────────────────────

  let trustNodes: TrustNode[] = $state([]);
  let trustEdges: TrustEdge[] = $state([]);
  let consensusRounds: ConsensusRound[] = $state([]);
  let delegations: Delegation[] = $state([]);
  let sybilMetrics: SybilMetrics = $state({ total_delegations: 0, max_chain_depth: 0, unique_delegators: 0 });
  let error: string | null = $state(null);
  let activeTab: 'trust' | 'consensus' | 'sybil' | 'a2a' = $state('trust');

  // A2A discovery state (T-4.4.1)
  let discoveredAgents = $state<DiscoveredA2AAgent[]>([]);
  let a2aTasks = $state<A2ATask[]>([]);
  let discovering = $state(false);
  let sendingTask = $state(false);
  let sendTarget = $state('');
  let sendInput = $state('');

  // d3-force simulation state
  interface SimNode extends SimulationNodeDatum {
    id: string;
    name: string;
    convergence_level: number;
  }

  interface SimLink extends SimulationLinkDatum<SimNode> {
    trust_score: number;
  }

  let simNodes: SimNode[] = $state([]);
  let simLinks: SimLink[] = $state([]);
  let ticked = $state(0);
  let viewBox = $state({ x: -300, y: -200, w: 600, h: 400 });

  // ── Data loading ─────────────────────────────────────────────────

  onMount(() => {
    loadAll();

    // T-5.9.1: Wire A2ATaskUpdate WS event to refresh A2A tasks.
    const unsub = wsStore.on('A2ATaskUpdate', () => { loadA2A(); });
    return () => unsub();
  });

  async function loadAll() {
    try {
      const client = await getGhostClient();
      const [trustRes, consensusRes, delegationsRes] = await Promise.all([
        client.mesh.trustGraph(),
        client.mesh.consensus(),
        client.mesh.delegations(),
      ]);
      trustNodes = trustRes.nodes ?? [];
      trustEdges = trustRes.edges ?? [];
      consensusRounds = consensusRes.rounds ?? [];
      delegations = delegationsRes.delegations ?? [];
      sybilMetrics = delegationsRes.sybil_metrics ?? sybilMetrics;
      initSimulation();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load orchestration data';
    }
  }

  async function loadA2A() {
    try {
      const client = await getGhostClient();
      const [agentsRes, tasksRes] = await Promise.all([
        client.a2a.discoverAgents(),
        client.a2a.listTasks(),
      ]);
      discoveredAgents = agentsRes.agents ?? [];
      a2aTasks = tasksRes.tasks ?? [];
    } catch (e: unknown) {
      // T-5.9.2: Surface error for A2A load failures.
      error = e instanceof Error ? e.message : 'Failed to load A2A data';
    }
  }

  async function discoverAgents() {
    discovering = true;
    try {
      const client = await getGhostClient();
      const result = await client.a2a.discoverAgents();
      discoveredAgents = result.agents ?? [];
      const tasks = await client.a2a.listTasks();
      a2aTasks = tasks.tasks ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Agent discovery failed';
    } finally {
      discovering = false;
    }
  }

  async function sendA2ATask() {
    if (!sendTarget.trim() || !sendInput.trim()) return;
    sendingTask = true;
    try {
      const client = await getGhostClient();
      await client.a2a.sendTask({
        target_url: sendTarget.trim(),
        input: JSON.parse(sendInput.trim()),
      });
      sendTarget = '';
      sendInput = '';
      await loadA2A();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to send A2A task';
    } finally {
      sendingTask = false;
    }
  }

  function initSimulation() {
    const sNodes: SimNode[] = trustNodes.map(n => ({
      id: n.id,
      name: n.name,
      convergence_level: n.convergence_level,
    }));

    const sLinks: SimLink[] = trustEdges.map(e => ({
      source: e.source,
      target: e.target,
      trust_score: e.trust_score,
    }));

    forceSimulation(sNodes)
      .force('link', forceLink<SimNode, SimLink>(sLinks).id(d => d.id).distance(100))
      .force('charge', forceManyBody().strength(-200))
      .force('center', forceCenter(0, 0))
      .force('collide', forceCollide(40))
      .on('tick', () => {
        simNodes = sNodes;
        simLinks = sLinks;
        ticked++;
      })
      .alpha(1)
      .restart();
  }

  const LEVEL_COLORS = [
    'var(--color-severity-normal)',
    'var(--color-severity-soft)',
    'var(--color-severity-active)',
    'var(--color-severity-hard)',
    'var(--color-severity-external)',
  ];

  function levelColor(level: number): string {
    return LEVEL_COLORS[Math.min(level, 4)];
  }

  function linkSource(link: SimLink): SimNode | undefined {
    return typeof link.source === 'object' ? (link.source as SimNode) : undefined;
  }

  function linkTarget(link: SimLink): SimNode | undefined {
    return typeof link.target === 'object' ? (link.target as SimNode) : undefined;
  }

  // ── Touch pan/zoom for trust graph ──────────────────────────────
  let touchStart: { x: number; y: number; dist: number } | null = null;

  function handleGraphTouchStart(e: TouchEvent) {
    if (e.touches.length === 1) {
      touchStart = { x: e.touches[0].clientX, y: e.touches[0].clientY, dist: 0 };
    } else if (e.touches.length === 2) {
      const dx = e.touches[1].clientX - e.touches[0].clientX;
      const dy = e.touches[1].clientY - e.touches[0].clientY;
      touchStart = {
        x: (e.touches[0].clientX + e.touches[1].clientX) / 2,
        y: (e.touches[0].clientY + e.touches[1].clientY) / 2,
        dist: Math.hypot(dx, dy),
      };
    }
  }

  function handleGraphTouchMove(e: TouchEvent) {
    if (!touchStart) return;
    e.preventDefault();
    if (e.touches.length === 1) {
      const dx = e.touches[0].clientX - touchStart.x;
      const dy = e.touches[0].clientY - touchStart.y;
      const scale = viewBox.w / 600;
      viewBox = { ...viewBox, x: viewBox.x - dx * scale, y: viewBox.y - dy * scale };
      touchStart = { x: e.touches[0].clientX, y: e.touches[0].clientY, dist: 0 };
    } else if (e.touches.length === 2 && touchStart.dist > 0) {
      const dx = e.touches[1].clientX - e.touches[0].clientX;
      const dy = e.touches[1].clientY - e.touches[0].clientY;
      const newDist = Math.hypot(dx, dy);
      const ratio = touchStart.dist / newDist;
      const cx = viewBox.x + viewBox.w / 2;
      const cy = viewBox.y + viewBox.h / 2;
      const nw = Math.max(200, Math.min(2000, viewBox.w * ratio));
      const nh = nw * (viewBox.h / viewBox.w);
      viewBox = { x: cx - nw / 2, y: cy - nh / 2, w: nw, h: nh };
      touchStart.dist = newDist;
    }
  }

  function handleGraphTouchEnd() {
    touchStart = null;
  }
</script>

<svelte:head>
  <title>Orchestration | ADE</title>
</svelte:head>

<div class="orchestration-page">
  <header class="page-header">
    <h1>Orchestration</h1>
    <p class="subtitle">Multi-agent trust graph, consensus, and sybil resistance</p>
  </header>

  {#if error}
    <p class="error-msg">{error}</p>
  {/if}

  <div class="tab-bar" role="tablist">
    <button role="tab" class:active={activeTab === 'trust'} aria-selected={activeTab === 'trust'} onclick={() => activeTab = 'trust'}>Trust Graph</button>
    <button role="tab" class:active={activeTab === 'consensus'} aria-selected={activeTab === 'consensus'} onclick={() => activeTab = 'consensus'}>Consensus</button>
    <button role="tab" class:active={activeTab === 'sybil'} aria-selected={activeTab === 'sybil'} onclick={() => activeTab = 'sybil'}>Sybil Resistance</button>
    <button role="tab" class:active={activeTab === 'a2a'} aria-selected={activeTab === 'a2a'} onclick={() => { activeTab = 'a2a'; loadA2A(); }}>A2A Discovery</button>
  </div>

  <div class="tab-content" role="tabpanel">
    {#if activeTab === 'trust'}
      <div class="trust-graph-container">
        {#if simNodes.length === 0}
          <p class="empty">No agents registered. Trust graph requires multiple agents.</p>
        {:else}
          <svg viewBox="{viewBox.x} {viewBox.y} {viewBox.w} {viewBox.h}" class="graph-svg"
            ontouchstart={handleGraphTouchStart}
            ontouchmove={handleGraphTouchMove}
            ontouchend={handleGraphTouchEnd}>
            <defs>
              <marker id="arrow-trust" markerWidth="8" markerHeight="6" refX="7" refY="3" orient="auto">
                <polygon points="0 0, 8 3, 0 6" fill="var(--color-border-default)" />
              </marker>
            </defs>

            {#each simLinks as link}
              {@const s = linkSource(link)}
              {@const t = linkTarget(link)}
              {#if s && t && s.x != null && t.x != null}
                <line
                  x1={s.x} y1={s.y}
                  x2={t.x} y2={t.y}
                  stroke="var(--color-border-default)"
                  stroke-width={Math.max(1, link.trust_score * 3)}
                  marker-end="url(#arrow-trust)"
                  opacity="0.5"
                />
                <text
                  x={(s.x! + t.x!) / 2}
                  y={(s.y! + t.y!) / 2 - 5}
                  fill="var(--color-text-muted)"
                  font-size="8"
                  text-anchor="middle"
                  font-family="var(--font-family-mono)"
                >
                  {link.trust_score.toFixed(2)}
                </text>
              {/if}
            {/each}

            {#each simNodes as node}
              {#if node.x != null && node.y != null}
                <g transform="translate({node.x}, {node.y})">
                  <circle
                    r="24"
                    fill="var(--color-bg-elevated-1)"
                    stroke={levelColor(node.convergence_level)}
                    stroke-width="2.5"
                  />
                  <text
                    y="-2"
                    text-anchor="middle"
                    fill="var(--color-text-primary)"
                    font-size="9"
                    font-weight="600"
                  >
                    {node.name.length > 10 ? node.name.slice(0, 9) + '…' : node.name}
                  </text>
                  <text
                    y="10"
                    text-anchor="middle"
                    fill="var(--color-text-muted)"
                    font-size="7"
                    font-family="var(--font-family-mono)"
                  >
                    L{node.convergence_level}
                  </text>
                </g>
              {/if}
            {/each}
          </svg>
        {/if}
      </div>

    {:else if activeTab === 'consensus'}
      <div class="consensus-panel">
        {#if consensusRounds.length === 0}
          <p class="empty">No consensus rounds found.</p>
        {:else}
          <table class="data-table">
            <thead>
              <tr>
                <th>Proposal</th>
                <th>Status</th>
                <th>Approvals</th>
                <th>Rejections</th>
                <th>Threshold</th>
                <th>Progress</th>
              </tr>
            </thead>
            <tbody>
              {#each consensusRounds as round}
                <tr>
                  <td class="mono">{round.proposal_id.slice(0, 8)}…</td>
                  <td>
                    <span class="status-badge" class:approved={round.status === 'approved'} class:rejected={round.status === 'rejected'} class:pending={round.status === 'pending'}>
                      {round.status}
                    </span>
                  </td>
                  <td class="mono">{round.approvals}</td>
                  <td class="mono">{round.rejections}</td>
                  <td class="mono">{round.threshold}</td>
                  <td>
                    <div class="progress-bar">
                      <div class="progress-fill" style="width: {Math.min((round.approvals / Math.max(round.threshold, 1)) * 100, 100)}%"></div>
                    </div>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>

    {:else if activeTab === 'sybil'}
      <div class="sybil-panel">
        <div class="metric-cards">
          <div class="metric-card">
            <span class="metric-value mono">{sybilMetrics.total_delegations}</span>
            <span class="metric-label">Total Delegations</span>
          </div>
          <div class="metric-card">
            <span class="metric-value mono">{sybilMetrics.max_chain_depth}</span>
            <span class="metric-label">Max Chain Depth</span>
          </div>
          <div class="metric-card">
            <span class="metric-value mono">{sybilMetrics.unique_delegators}</span>
            <span class="metric-label">Unique Delegators</span>
          </div>
        </div>

        {#if delegations.length === 0}
          <p class="empty">No active delegations.</p>
        {:else}
          <table class="data-table">
            <thead>
              <tr>
                <th>Delegator</th>
                <th>Delegate</th>
                <th>Scope</th>
                <th>Trust</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {#each delegations as d}
                <tr>
                  <td class="mono">{d.delegator_id.slice(0, 8)}…</td>
                  <td class="mono">{d.delegate_id.slice(0, 8)}…</td>
                  <td>{d.scope || '—'}</td>
                  <td>{d.state || '—'}</td>
                  <td class="mono">{d.created_at ? new Date(d.created_at).toLocaleDateString() : '—'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>

    {:else if activeTab === 'a2a'}
      <div class="a2a-panel">
        <div class="a2a-controls">
          <button class="discover-btn" onclick={discoverAgents} disabled={discovering}>
            {discovering ? 'Discovering...' : 'Discover Agents'}
          </button>
        </div>

        <section class="a2a-section">
          <h2>Discovered Agents ({discoveredAgents.length})</h2>
          {#if discoveredAgents.length === 0}
            <p class="empty">No agents discovered yet. Click "Discover Agents" to probe mesh peers.</p>
          {:else}
            <div class="agent-grid">
              {#each discoveredAgents as agent}
                <A2AAgentCard {agent} />
              {/each}
            </div>
          {/if}
        </section>

        <section class="a2a-section">
          <h2>Send Task</h2>
          <div class="send-form">
            <input type="text" bind:value={sendTarget} placeholder="Target agent URL (e.g. http://host/.well-known/agent.json)" class="send-input" />
            <textarea bind:value={sendInput} placeholder={'Task input JSON (e.g. {"text": "Hello"})'} class="send-textarea" rows="3"></textarea>
            <button class="send-btn" onclick={sendA2ATask} disabled={sendingTask || !sendTarget.trim() || !sendInput.trim()}>
              {sendingTask ? 'Sending...' : 'Send Task'}
            </button>
          </div>
        </section>

        <section class="a2a-section">
          <h2>In-Flight Tasks ({a2aTasks.length})</h2>
          <A2ATaskTracker />
        </section>
      </div>
    {/if}
  </div>
</div>

<style>
  .orchestration-page {
    padding: var(--spacing-6);
    max-width: 1200px;
  }

  .page-header {
    margin-bottom: var(--spacing-6);
  }

  .page-header h1 {
    font-size: var(--font-size-2xl);
    font-weight: 700;
    color: var(--color-text-primary);
  }

  .subtitle {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin-top: var(--spacing-1);
  }

  .tab-bar {
    display: flex;
    gap: var(--spacing-1);
    margin-bottom: var(--spacing-4);
    border-bottom: 1px solid var(--color-border-default);
    padding-bottom: var(--spacing-1);
  }

  .tab-bar button {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    padding: var(--spacing-2) var(--spacing-3);
    cursor: pointer;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    font-weight: 500;
    transition: color 0.1s, border-color 0.1s;
  }

  .tab-bar button:hover {
    color: var(--color-text-primary);
  }

  .tab-bar button.active {
    color: var(--color-interactive-primary);
    border-bottom-color: var(--color-interactive-primary);
  }

  .graph-svg {
    width: 100%;
    height: 450px;
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .data-table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-sm);
  }

  .data-table th {
    text-align: left;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-1);
    color: var(--color-text-muted);
    font-weight: 600;
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    border-bottom: 1px solid var(--color-border-default);
  }

  .data-table td {
    padding: var(--spacing-2) var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
    color: var(--color-text-primary);
  }

  .status-badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: 500;
  }

  .status-badge.approved {
    background: var(--color-severity-normal);
    color: var(--color-text-inverse);
  }

  .status-badge.rejected {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }

  .status-badge.pending {
    background: var(--color-severity-soft);
    color: var(--color-text-inverse);
  }

  .progress-bar {
    width: 100%;
    height: 8px;
    background: var(--color-bg-elevated-2);
    border-radius: 4px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--color-severity-normal);
    border-radius: 4px;
    transition: width 0.2s;
  }

  .metric-cards {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-6);
  }

  .metric-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    text-align: center;
  }

  .metric-value {
    display: block;
    font-size: var(--font-size-2xl);
    font-weight: 700;
    color: var(--color-text-primary);
    font-variant-numeric: tabular-nums;
  }

  .metric-label {
    display: block;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-top: var(--spacing-1);
  }

  .mono {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .empty {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .error-msg {
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    margin-bottom: var(--spacing-4);
  }

  .a2a-panel {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-6);
  }

  .a2a-controls {
    display: flex;
    gap: var(--spacing-3);
  }

  .discover-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .discover-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .a2a-section {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .a2a-section h2 {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    margin: 0;
  }

  .agent-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: var(--spacing-3);
  }

  .send-form {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
    max-width: 600px;
  }

  .send-input, .send-textarea {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
  }

  .send-btn {
    align-self: flex-start;
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-2) var(--spacing-4);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .send-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
</style>
