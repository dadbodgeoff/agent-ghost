<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import GoalCard from '../../components/GoalCard.svelte';
  import { wsStore } from '$lib/stores/websocket.svelte';

  interface Proposal {
    id: string;
    agent_id: string;
    session_id: string;
    proposer_type: string;
    operation: string;
    target_type: string;
    decision: string | null;
    dimension_scores: Record<string, any>;
    flags: string[];
    created_at: string;
    resolved_at: string | null;
  }

  let proposals: Proposal[] = $state([]);
  let loading = $state(true);
  let error = $state('');

  // Filter state (T-2.4.1)
  let statusFilter = $state<string>('pending');
  let agentFilter = $state<string>('');

  const statusTabs = ['pending', 'approved', 'rejected', 'all'] as const;

  async function loadProposals() {
    try {
      loading = true;
      error = '';
      const params = new URLSearchParams();
      if (statusFilter !== 'all') params.set('status', statusFilter);
      if (agentFilter) params.set('agent_id', agentFilter);
      params.set('page_size', '100');

      const data = await api.get(`/api/goals?${params}`);
      proposals = data?.proposals ?? [];
    } catch (e: any) {
      error = e.message || 'Failed to load goals';
    }
    loading = false;
  }

  onMount(async () => {
    await loadProposals();

    // Live updates: handle concurrent approve/reject via WebSocket (T-2.4.2)
    const unsub = wsStore.on('ProposalDecision', (event: any) => {
      proposals = proposals.map(p =>
        p.id === event.proposal_id ? { ...p, decision: event.decision, resolved_at: new Date().toISOString() } : p
      );
    });

    return () => unsub();
  });

  function switchTab(tab: string) {
    statusFilter = tab;
    loadProposals();
  }

  function filterByAgent() {
    loadProposals();
  }

  async function handleApprove(id: string) {
    try {
      await api.post(`/api/goals/${id}/approve`);
      proposals = proposals.map(p =>
        p.id === id ? { ...p, decision: 'approved', resolved_at: new Date().toISOString() } : p
      );
    } catch (e: any) {
      if (e.message?.includes('already resolved') || e.message?.includes('409')) {
        await loadProposals();
      }
    }
  }

  async function handleReject(id: string) {
    try {
      await api.post(`/api/goals/${id}/reject`);
      proposals = proposals.map(p =>
        p.id === id ? { ...p, decision: 'rejected', resolved_at: new Date().toISOString() } : p
      );
    } catch (e: any) {
      if (e.message?.includes('already resolved') || e.message?.includes('409')) {
        await loadProposals();
      }
    }
  }
</script>

<h1 class="page-title">Goals</h1>

<!-- Status Tabs -->
<div class="status-tabs" role="tablist">
  {#each statusTabs as tab}
    <button
      class="tab"
      class:active={statusFilter === tab}
      role="tab"
      aria-selected={statusFilter === tab}
      onclick={() => switchTab(tab)}
    >
      {tab.charAt(0).toUpperCase() + tab.slice(1)}
    </button>
  {/each}
</div>

<!-- Agent Filter -->
<div class="filter-row">
  <input
    type="text"
    class="agent-filter"
    placeholder="Filter by agent ID..."
    bind:value={agentFilter}
    onchange={filterByAgent}
  />
</div>

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <button onclick={() => loadProposals()}>Retry</button>
  </div>
{:else if proposals.length === 0}
  <div class="empty-state">
    <p>No {statusFilter === 'all' ? '' : statusFilter} proposals found.</p>
  </div>
{:else}
  <div class="proposals-list">
    {#each proposals as proposal (proposal.id)}
      <div class="proposal-row">
        <GoalCard
          goal={{
            id: proposal.id,
            description: `${proposal.operation} → ${proposal.target_type}`,
            decision: proposal.decision ?? 'pending',
            created_at: proposal.created_at,
            agent_id: proposal.agent_id,
          }}
          onapprove={handleApprove}
          onreject={handleReject}
        />
        <a href="/goals/{proposal.id}" class="detail-link">View Details →</a>
      </div>
    {/each}
  </div>
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-4);
  }

  .status-tabs {
    display: flex;
    gap: var(--spacing-1);
    margin-bottom: var(--spacing-4);
    border-bottom: 1px solid var(--color-border-primary);
    padding-bottom: var(--spacing-1);
  }

  .tab {
    padding: var(--spacing-2) var(--spacing-4);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-tertiary);
    cursor: pointer;
    transition: all var(--duration-fast) var(--easing-default);
  }

  .tab:hover {
    color: var(--color-text-primary);
  }

  .tab.active {
    color: var(--color-interactive-primary);
    border-bottom-color: var(--color-interactive-primary);
  }

  .filter-row {
    margin-bottom: var(--spacing-4);
  }

  .agent-filter {
    width: 100%;
    max-width: 300px;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-secondary);
    border: 1px solid var(--color-border-primary);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
  }

  .agent-filter::placeholder {
    color: var(--color-text-quaternary);
  }

  .proposals-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .proposal-row {
    position: relative;
  }

  .detail-link {
    display: inline-block;
    margin-top: var(--spacing-1);
    font-size: var(--font-size-xs);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .detail-link:hover {
    text-decoration: underline;
  }

  .skeleton-block {
    height: 200px;
    background: var(--color-bg-secondary);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }

  .empty-state, .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-tertiary);
  }

  .error-state button {
    margin-top: var(--spacing-4);
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
  }
</style>
