<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import {
    GhostAPIError,
    type ActiveGoal,
    type GoalDecisionRequest,
    type Proposal,
    type ProposalDetail,
  } from '@ghost/sdk';
  import { wsStore, type WsMessage } from '$lib/stores/websocket.svelte';

  const PENDING_STATUS = 'pending_review';

  let activeGoals = $state<ActiveGoal[]>([]);
  let proposals = $state<Proposal[]>([]);
  let loading = $state(true);
  let error = $state('');
  let notice = $state('');
  let actionLoading = $state<string | null>(null);
  let activeTab = $state<'pending' | 'history'>('pending');
  let agentFilter = $state('');
  let appliedAgentFilter = $state('');
  let unsubs: Array<() => void> = [];

  let pendingProposals = $derived(
    proposals.filter((proposal) => proposalStatus(proposal) === PENDING_STATUS),
  );
  let historyProposals = $derived(
    proposals.filter((proposal) => proposalStatus(proposal) !== PENDING_STATUS),
  );
  let visibleProposals = $derived(
    activeTab === 'pending' ? pendingProposals : historyProposals,
  );

  onMount(() => {
    void loadData();

    unsubs = [
      wsStore.on('ProposalUpdated', (_msg: WsMessage) => {
        void loadData();
      }),
      wsStore.onResync(() => {
        void loadData();
      }),
    ];
  });

  onDestroy(() => {
    for (const unsub of unsubs) {
      unsub();
    }
  });

  async function loadData() {
    loading = true;
    error = '';

    try {
      const agentId = appliedAgentFilter || undefined;
      const [active, proposalRows] = await Promise.all([
        fetchActiveGoals(agentId),
        fetchAllProposals(agentId),
      ]);
      activeGoals = active;
      proposals = proposalRows;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load proposals';
    } finally {
      loading = false;
    }
  }

  async function fetchActiveGoals(agentId?: string): Promise<ActiveGoal[]> {
    const client = await getGhostClient();
    const response = await client.goals.listActive({
      agent_id: agentId,
      page: 1,
      page_size: 200,
    });
    return response.goals ?? [];
  }

  async function fetchAllProposals(agentId?: string): Promise<Proposal[]> {
    const client = await getGhostClient();
    const collected: Proposal[] = [];
    let page = 1;
    let total = 0;

    do {
      const response = await client.goals.list({
        agent_id: agentId,
        page,
        page_size: 200,
      });
      collected.push(...(response.proposals ?? []));
      total = response.total ?? collected.length;
      if ((response.proposals ?? []).length === 0) {
        break;
      }
      page += 1;
    } while (collected.length < total);

    return collected;
  }

  async function handleDecision(proposalId: string, action: 'approve' | 'reject') {
    actionLoading = proposalId;
    error = '';
    notice = '';

    try {
      const client = await getGhostClient();
      const detail = await client.goals.get(proposalId);
      const request = decisionRequest(detail);
      if (!request) {
        error = 'Proposal detail is missing required lineage or revision fields';
        return;
      }
      if (action === 'approve') {
        await client.goals.approve(proposalId, request);
      } else {
        await client.goals.reject(proposalId, request);
      }
      notice = `Proposal ${proposalId.slice(0, 8)}... ${action === 'approve' ? 'approved' : 'rejected'}.`;
      await loadData();
    } catch (e: unknown) {
      if (isStaleDecisionError(e)) {
        notice = `Proposal ${proposalId.slice(0, 8)}... is stale and must be re-reviewed before a decision is applied.`;
        await loadData();
      } else {
        error = e instanceof Error ? e.message : `Failed to ${action} proposal`;
      }
    } finally {
      actionLoading = null;
    }
  }

  function applyAgentFilter() {
    appliedAgentFilter = agentFilter.trim();
    void loadData();
  }

  function clearAgentFilter() {
    agentFilter = '';
    appliedAgentFilter = '';
    void loadData();
  }

  function decisionRequest(detail: ProposalDetail): GoalDecisionRequest | null {
    if (
      !detail.current_state ||
      !detail.lineage_id ||
      !detail.subject_key ||
      !detail.reviewed_revision
    ) {
      return null;
    }

    return {
      expectedState: detail.current_state,
      expectedLineageId: detail.lineage_id,
      expectedSubjectKey: detail.subject_key,
      expectedReviewedRevision: detail.reviewed_revision,
    };
  }

  function isStaleDecisionError(errorValue: unknown): boolean {
    return Boolean(
      errorValue instanceof GhostAPIError && errorValue.code?.startsWith('STALE_DECISION_'),
    );
  }

  function proposalStatus(proposal: Pick<Proposal, 'status' | 'current_state' | 'decision'>): string {
    return proposal.status ?? proposal.current_state ?? fallbackStatusFromDecision(proposal.decision);
  }

  function fallbackStatusFromDecision(decision: string | null | undefined): string {
    switch (decision) {
      case 'approved':
        return 'approved';
      case 'rejected':
        return 'rejected';
      case 'Superseded':
        return 'superseded';
      case 'TimedOut':
        return 'timed_out';
      case 'AutoApproved':
      case 'ApprovedWithFlags':
        return 'auto_applied';
      case 'AutoRejected':
        return 'auto_rejected';
      default:
        return PENDING_STATUS;
    }
  }

  function statusLabel(status: string): string {
    return status.replaceAll('_', ' ');
  }

  function statusClass(status: string): string {
    if (status === PENDING_STATUS) return 'pending';
    if (status === 'approved') return 'approved';
    if (status === 'rejected') return 'rejected';
    return 'history';
  }

  function relativeTime(iso: string): string {
    try {
      const diffMs = Date.now() - new Date(iso).getTime();
      const minutes = Math.max(0, Math.floor(diffMs / 60_000));
      if (minutes < 1) return 'just now';
      if (minutes < 60) return `${minutes}m ago`;
      const hours = Math.floor(minutes / 60);
      if (hours < 24) return `${hours}h ago`;
      return `${Math.floor(hours / 24)}d ago`;
    } catch {
      return '';
    }
  }

  function scoreEntries(proposal: Proposal): Array<[string, number]> {
    return Object.entries(proposal.dimension_scores ?? {}).sort(([left], [right]) =>
      left.localeCompare(right),
    );
  }
</script>

<svelte:head>
  <title>Proposals</title>
</svelte:head>

<div class="page-shell">
  <header class="page-header">
    <div>
      <p class="eyebrow">ADE Review Queue</p>
      <h1>Proposals</h1>
      <p class="subtitle">
        Canonical proposal review surface for the gateway lifecycle contract.
      </p>
    </div>
    <div class="tab-bar" role="tablist" aria-label="Proposal status views">
      <button
        type="button"
        role="tab"
        class:active={activeTab === 'pending'}
        aria-selected={activeTab === 'pending'}
        onclick={() => (activeTab = 'pending')}
      >
        Pending
        <span>{pendingProposals.length}</span>
      </button>
      <button
        type="button"
        role="tab"
        class:active={activeTab === 'history'}
        aria-selected={activeTab === 'history'}
        onclick={() => (activeTab = 'history')}
      >
        History
        <span>{historyProposals.length}</span>
      </button>
    </div>
  </header>

  <section class="active-goals card-shell">
    <div class="section-header">
      <div>
        <p class="eyebrow">Canonical Goal State</p>
        <h2>Active Goals</h2>
      </div>
      <span class="count-pill">{activeGoals.length}</span>
    </div>

    {#if activeGoals.length === 0}
      <p class="section-empty">No active goals are currently materialized.</p>
    {:else}
      <div class="active-goal-list">
        {#each activeGoals as goal (goal.id)}
          <article class="active-goal-card">
            <div class="active-goal-card__header">
              <div>
                <p class="active-goal-card__title">{goal.goal_text}</p>
                <p class="active-goal-card__meta">
                  {goal.origin} · {goal.scope} · {goal.state.replaceAll('_', ' ')}
                </p>
              </div>
              <span class="mono">{goal.agent_id.slice(0, 8)}</span>
            </div>
            <dl class="active-goal-card__details">
              <div>
                <dt>Subject</dt>
                <dd class="mono">{goal.subject_key}</dd>
              </div>
              <div>
                <dt>Revision</dt>
                <dd class="mono">{goal.reviewed_revision}</dd>
              </div>
              <div>
                <dt>Proposal</dt>
                <dd><a href={`/goals/${goal.proposal_id}`}>{goal.proposal_id.slice(0, 8)}...</a></dd>
              </div>
            </dl>
          </article>
        {/each}
      </div>
    {/if}
  </section>

  <form
    class="filter-bar"
    onsubmit={(event) => {
      event.preventDefault();
      applyAgentFilter();
    }}
  >
    <label class="filter-field">
      <span>Agent filter</span>
      <input
        type="text"
        bind:value={agentFilter}
        placeholder="agent-1234"
        autocomplete="off"
      />
    </label>
    <div class="filter-actions">
      <button type="submit">Apply</button>
      {#if appliedAgentFilter}
        <button type="button" class="secondary" onclick={clearAgentFilter}>Clear</button>
      {/if}
    </div>
  </form>

  {#if notice}
    <div class="banner notice" role="status">{notice}</div>
  {/if}

  {#if error}
    <div class="banner error" role="alert">{error}</div>
  {/if}

  {#if loading}
    <div class="empty-state">Loading goals and proposals...</div>
  {:else if visibleProposals.length === 0}
    <div class="empty-state">
      {#if activeTab === 'pending'}
        No proposals are waiting for review.
      {:else}
        No proposal history matched the current filter.
      {/if}
    </div>
  {:else}
    <div class="proposal-list">
      {#each visibleProposals as proposal (proposal.id)}
        {@const status = proposalStatus(proposal)}
        {@const pending = status === PENDING_STATUS}
        <article class="proposal-card">
          <div class="proposal-card__header">
            <div>
              <p class="proposal-card__title">{proposal.operation} -> {proposal.target_type}</p>
              <p class="proposal-card__subtitle">
                {proposal.proposer_type} proposal for <code>{proposal.agent_id}</code>
              </p>
            </div>
            <span class={`status-badge ${statusClass(status)}`}>{statusLabel(status)}</span>
          </div>

          <dl class="meta-grid">
            <div>
              <dt>Created</dt>
              <dd title={proposal.created_at}>{relativeTime(proposal.created_at)}</dd>
            </div>
            <div>
              <dt>Session</dt>
              <dd><code>{proposal.session_id}</code></dd>
            </div>
            <div>
              <dt>Decision</dt>
              <dd>{proposal.decision ?? 'HumanReviewRequired'}</dd>
            </div>
            <div>
              <dt>Resolved</dt>
              <dd>{proposal.resolved_at ? relativeTime(proposal.resolved_at) : 'Not yet'}</dd>
            </div>
          </dl>

          {#if proposal.flags.length > 0}
            <div class="pill-row">
              {#each proposal.flags as flag}
                <span class="pill warning">{flag}</span>
              {/each}
            </div>
          {/if}

          {#if scoreEntries(proposal).length > 0}
            <div class="pill-row">
              {#each scoreEntries(proposal) as [label, value]}
                <span class="pill">{label}: {value.toFixed(2)}</span>
              {/each}
            </div>
          {/if}

          <div class="card-actions">
            <a class="detail-link" href={`/goals/${proposal.id}`}>Review detail</a>
            {#if pending}
              <div class="decision-actions">
                <button
                  type="button"
                  class="secondary"
                  disabled={actionLoading === proposal.id}
                  onclick={() => handleDecision(proposal.id, 'reject')}
                >
                  {actionLoading === proposal.id ? 'Working...' : 'Reject'}
                </button>
                <button
                  type="button"
                  disabled={actionLoading === proposal.id}
                  onclick={() => handleDecision(proposal.id, 'approve')}
                >
                  {actionLoading === proposal.id ? 'Working...' : 'Approve'}
                </button>
              </div>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .page-shell {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-4);
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-4);
    align-items: end;
    flex-wrap: wrap;
  }

  .eyebrow {
    margin: 0 0 var(--spacing-1);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  h1 {
    margin: 0;
    font-size: var(--font-size-xl);
  }

  .subtitle {
    margin: var(--spacing-1) 0 0;
    color: var(--color-text-muted);
    max-width: 48rem;
  }

  .card-shell {
    padding: var(--spacing-4);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-3);
    align-items: start;
    margin-bottom: var(--spacing-3);
  }

  h2 {
    margin: 0;
    font-size: var(--font-size-lg);
  }

  .count-pill {
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: 999px;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    font-family: var(--font-family-mono);
  }

  .section-empty {
    margin: 0;
    color: var(--color-text-muted);
  }

  .active-goal-list {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr));
    gap: var(--spacing-3);
  }

  .active-goal-card {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    padding: var(--spacing-3);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-2);
  }

  .active-goal-card__header {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-3);
    align-items: start;
  }

  .active-goal-card__title {
    margin: 0;
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
  }

  .active-goal-card__meta {
    margin: var(--spacing-1) 0 0;
    color: var(--color-text-muted);
  }

  .active-goal-card__details {
    margin: 0;
    display: grid;
    gap: var(--spacing-2);
  }

  .active-goal-card__details div {
    display: grid;
    gap: 2px;
  }

  .active-goal-card__details dt {
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-muted);
  }

  .active-goal-card__details dd {
    margin: 0;
    word-break: break-word;
  }

  .tab-bar {
    display: flex;
    gap: var(--spacing-2);
  }

  .tab-bar button {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    border-radius: 999px;
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
    color: var(--color-text-muted);
  }

  .tab-bar button.active {
    border-color: var(--color-interactive-primary);
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-interactive-primary) 12%, transparent);
  }

  .tab-bar span {
    font-family: var(--font-family-mono);
  }

  .filter-bar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-3);
    align-items: end;
    padding: var(--spacing-3);
    border-radius: var(--radius-md);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
  }

  .filter-field {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
    min-width: min(24rem, 100%);
  }

  .filter-field span {
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-muted);
  }

  .filter-field input {
    padding: var(--spacing-2) var(--spacing-3);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
  }

  .filter-actions {
    display: flex;
    gap: var(--spacing-2);
  }

  .banner {
    padding: var(--spacing-3);
    border-radius: var(--radius-md);
    border: 1px solid transparent;
  }

  .banner.notice {
    border-color: color-mix(in srgb, var(--color-interactive-primary) 25%, transparent);
    background: color-mix(in srgb, var(--color-interactive-primary) 10%, transparent);
  }

  .banner.error {
    border-color: color-mix(in srgb, var(--color-severity-hard) 35%, transparent);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
  }

  .empty-state {
    padding: var(--spacing-8);
    text-align: center;
    border-radius: var(--radius-md);
    border: 1px dashed var(--color-border-default);
    color: var(--color-text-muted);
    background: var(--color-bg-elevated-1);
  }

  .proposal-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .proposal-card {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
    padding: var(--spacing-4);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
  }

  .proposal-card__header {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-3);
    align-items: start;
  }

  .proposal-card__title {
    margin: 0;
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
  }

  .proposal-card__subtitle {
    margin: var(--spacing-1) 0 0;
    color: var(--color-text-muted);
  }

  .proposal-card__subtitle code,
  .meta-grid code {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .status-badge {
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: 999px;
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    white-space: nowrap;
  }

  .status-badge.pending {
    background: color-mix(in srgb, var(--color-severity-soft) 18%, transparent);
    color: var(--color-severity-soft);
  }

  .status-badge.approved {
    background: color-mix(in srgb, var(--color-severity-normal) 18%, transparent);
    color: var(--color-severity-normal);
  }

  .status-badge.rejected,
  .status-badge.history {
    background: color-mix(in srgb, var(--color-severity-hard) 12%, transparent);
    color: var(--color-text-primary);
  }

  .meta-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
    gap: var(--spacing-2) var(--spacing-3);
    margin: 0;
  }

  .meta-grid dt {
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-text-muted);
  }

  .meta-grid dd {
    margin: var(--spacing-1) 0 0;
  }

  .pill-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-2);
  }

  .pill {
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: 999px;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    font-size: var(--font-size-xs);
  }

  .pill.warning {
    border-color: color-mix(in srgb, var(--color-severity-soft) 30%, transparent);
  }

  .card-actions {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-3);
    align-items: center;
    flex-wrap: wrap;
  }

  .detail-link {
    color: var(--color-interactive-primary);
    text-decoration: none;
    font-weight: var(--font-weight-medium);
  }

  .decision-actions {
    display: flex;
    gap: var(--spacing-2);
  }

  .decision-actions button,
  .filter-actions button {
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    background: var(--color-interactive-primary);
    color: var(--color-text-inverse);
    padding: var(--spacing-2) var(--spacing-3);
    font-weight: var(--font-weight-medium);
  }

  .decision-actions button.secondary,
  .filter-actions button.secondary {
    background: transparent;
    color: var(--color-text-primary);
  }

  .decision-actions button:disabled {
    opacity: 0.65;
    cursor: wait;
  }

  @media (max-width: 720px) {
    .page-header,
    .proposal-card__header,
    .card-actions {
      flex-direction: column;
      align-items: stretch;
    }

    .decision-actions {
      width: 100%;
    }

    .decision-actions button {
      flex: 1;
    }
  }
</style>
