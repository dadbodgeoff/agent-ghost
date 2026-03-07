<script lang="ts">
  /**
   * Proposal Queue — gateway goal proposals that can be approved or rejected.
   *
   * This page intentionally reflects the real goals contract. It does not
   * invent approval types, risk levels, tool arguments, or mutable approval
   * payloads that the gateway does not actually own.
   */
  import { onMount, onDestroy } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';
  import type { Proposal, ProposalDetail } from '@ghost/sdk';
  import { wsStore, type WsMessage } from '$lib/stores/websocket.svelte';

  let proposals = $state<Proposal[]>([]);
  let proposalDetails = $state<Record<string, ProposalDetail>>({});
  let detailLoading = $state<Record<string, boolean>>({});
  let detailErrors = $state<Record<string, string>>({});
  let loading = $state(false);
  let error = $state('');
  let activeTab = $state<'pending' | 'history'>('pending');
  let expandedProposalId = $state<string | null>(null);
  let unsubs: Array<() => void> = [];

  let pendingProposals = $derived(
    proposals.filter((proposal) => proposal.decision === null && proposal.resolved_at === null),
  );
  let historyProposals = $derived(
    proposals.filter((proposal) => proposal.decision !== null || proposal.resolved_at !== null),
  );

  onMount(async () => {
    await loadProposals();

    unsubs.push(
      wsStore.on('ProposalDecision', (msg: WsMessage) => {
        const data = msg as { proposal_id?: string; decision?: 'approved' | 'rejected' };
        if (!data.proposal_id || !data.decision) {
          return;
        }

        const idx = proposals.findIndex((proposal) => proposal.id === data.proposal_id);
        if (idx < 0) {
          return;
        }

        proposals[idx] = {
          ...proposals[idx],
          decision: data.decision,
          resolved_at: new Date().toISOString(),
        };
        proposals = [...proposals];
      }),
      wsStore.on('Resync', () => {
        setTimeout(() => loadProposals(), Math.random() * 2000);
      }),
    );
  });

  onDestroy(() => {
    for (const unsub of unsubs) {
      unsub();
    }
  });

  async function loadProposals() {
    loading = true;
    error = '';

    try {
      const client = await getGhostClient();
      const data = await client.goals.list();
      proposals = data.proposals ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load proposals';
    }

    loading = false;
  }

  async function approveProposal(id: string) {
    try {
      const client = await getGhostClient();
      await client.goals.approve(id);
      markProposalResolved(id, 'approved');
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to approve proposal';
    }
  }

  async function rejectProposal(id: string) {
    try {
      const client = await getGhostClient();
      await client.goals.reject(id);
      markProposalResolved(id, 'rejected');
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to reject proposal';
    }
  }

  function markProposalResolved(id: string, decision: 'approved' | 'rejected') {
    const idx = proposals.findIndex((proposal) => proposal.id === id);
    if (idx < 0) {
      return;
    }

    proposals[idx] = {
      ...proposals[idx],
      decision,
      resolved_at: new Date().toISOString(),
    };
    proposals = [...proposals];
  }

  async function toggleProposalDetail(id: string) {
    if (expandedProposalId === id) {
      expandedProposalId = null;
      return;
    }

    expandedProposalId = id;

    if (proposalDetails[id] || detailLoading[id]) {
      return;
    }

    detailLoading = { ...detailLoading, [id]: true };
    detailErrors = { ...detailErrors, [id]: '' };

    try {
      const client = await getGhostClient();
      const detail = await client.goals.get(id);
      proposalDetails = { ...proposalDetails, [id]: detail };
    } catch (e: unknown) {
      detailErrors = {
        ...detailErrors,
        [id]: e instanceof Error ? e.message : 'Failed to load proposal detail',
      };
    } finally {
      detailLoading = { ...detailLoading, [id]: false };
    }
  }

  function proposalSummary(proposal: Proposal): string {
    return `${proposal.operation} on ${proposal.target_type}`;
  }

  function proposalStatus(proposal: Proposal): 'pending' | 'approved' | 'rejected' {
    if (proposal.decision === 'approved') {
      return 'approved';
    }
    if (proposal.decision === 'rejected') {
      return 'rejected';
    }
    return 'pending';
  }

  function formatJson(value: unknown): string {
    if (value === undefined) {
      return '';
    }
    return JSON.stringify(value, null, 2);
  }

  function relativeTime(iso: string | null): string {
    if (!iso) {
      return '';
    }

    try {
      const diff = Date.now() - new Date(iso).getTime();
      const mins = Math.floor(diff / 60000);
      if (mins < 1) return 'just now';
      if (mins < 60) return `${mins} minutes ago`;
      const hrs = Math.floor(mins / 60);
      if (hrs < 24) return `${hrs} hours ago`;
      return `${Math.floor(hrs / 24)} days ago`;
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

<div class="approvals-page">
  <div class="page-header">
    <div>
      <h1>Proposals</h1>
      <p class="subtitle">Gateway goal proposals awaiting operator review.</p>
    </div>
    <div class="tab-bar">
      <button class:active={activeTab === 'pending'} onclick={() => (activeTab = 'pending')}>
        Pending
        {#if pendingProposals.length > 0}
          <span class="count-badge">{pendingProposals.length}</span>
        {/if}
      </button>
      <button class:active={activeTab === 'history'} onclick={() => (activeTab = 'history')}>
        History
      </button>
    </div>
  </div>

  {#if error}
    <div class="error-bar">{error}</div>
  {/if}

  {#if loading}
    <div class="loading-state">Loading proposals...</div>
  {:else if activeTab === 'pending'}
    {#if pendingProposals.length === 0}
      <div class="empty-state">No pending proposals</div>
    {:else}
      <div class="proposal-list">
        {#each pendingProposals as proposal (proposal.id)}
          <div class="proposal-card">
            <div class="proposal-header">
              <span class="agent-name">{proposal.agent_id}</span>
              <span class="proposal-type">{proposal.proposer_type}</span>
              <span class="status-badge status-{proposalStatus(proposal)}">{proposalStatus(proposal)}</span>
            </div>

            <div class="proposal-body">
              <p class="proposal-desc">{proposalSummary(proposal)}</p>
              <dl class="proposal-meta">
                <div><dt>Target</dt><dd>{proposal.target_type}</dd></div>
                <div><dt>Session</dt><dd class="mono">{proposal.session_id}</dd></div>
                <div><dt>Created</dt><dd>{relativeTime(proposal.created_at)}</dd></div>
              </dl>

              {#if proposal.flags.length > 0}
                <div class="chip-row">
                  {#each proposal.flags as flag}
                    <span class="chip">{flag}</span>
                  {/each}
                </div>
              {/if}

              {#if scoreEntries(proposal).length > 0}
                <div class="scores">
                  {#each scoreEntries(proposal) as [name, value]}
                    <div class="score-row">
                      <span>{name}</span>
                      <span class="mono">{value.toFixed(2)}</span>
                    </div>
                  {/each}
                </div>
              {/if}

              {#if expandedProposalId === proposal.id}
                <div class="detail-panel">
                  {#if detailLoading[proposal.id]}
                    <div class="detail-state">Loading full proposal detail...</div>
                  {:else if detailErrors[proposal.id]}
                    <div class="detail-error">{detailErrors[proposal.id]}</div>
                  {:else if proposalDetails[proposal.id]}
                    <dl class="detail-meta">
                      <div><dt>Resolver</dt><dd>{proposalDetails[proposal.id].resolver ?? 'unresolved'}</dd></div>
                      <div><dt>Memory Links</dt><dd>{proposalDetails[proposal.id].cited_memory_ids.length}</dd></div>
                    </dl>

                    {#if proposalDetails[proposal.id].denial_reason}
                      <div class="detail-block">
                        <div class="detail-label">Denial Reason</div>
                        <div class="detail-text">{proposalDetails[proposal.id].denial_reason}</div>
                      </div>
                    {/if}

                    <div class="detail-block">
                      <div class="detail-label">Content</div>
                      <pre class="detail-json">{formatJson(proposalDetails[proposal.id].content)}</pre>
                    </div>

                    {#if proposalDetails[proposal.id].cited_memory_ids.length > 0}
                      <div class="detail-block">
                        <div class="detail-label">Cited Memory IDs</div>
                        <ul class="detail-list">
                          {#each proposalDetails[proposal.id].cited_memory_ids as memoryId}
                            <li class="mono">{memoryId}</li>
                          {/each}
                        </ul>
                      </div>
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>

            <div class="proposal-footer">
              <button class="btn-secondary" onclick={() => toggleProposalDetail(proposal.id)}>
                {expandedProposalId === proposal.id ? 'Hide Detail' : 'View Detail'}
              </button>
              <div class="action-buttons">
                <button class="btn-approve" onclick={() => approveProposal(proposal.id)}>Approve</button>
                <button class="btn-deny" onclick={() => rejectProposal(proposal.id)}>Reject</button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    {#if historyProposals.length === 0}
      <div class="empty-state">No proposal decision history</div>
    {:else}
      <div class="proposal-list">
        {#each historyProposals as proposal (proposal.id)}
          <div class="proposal-card history">
            <div class="proposal-header">
              <span class="agent-name">{proposal.agent_id}</span>
              <span class="proposal-type">{proposal.proposer_type}</span>
              <span class="status-badge status-{proposalStatus(proposal)}">{proposalStatus(proposal)}</span>
            </div>

            <div class="proposal-body">
              <p class="proposal-desc">{proposalSummary(proposal)}</p>
              <div class="history-meta">
                <span>Created {relativeTime(proposal.created_at)}</span>
                <span>Resolved {relativeTime(proposal.resolved_at)}</span>
              </div>

              {#if expandedProposalId === proposal.id}
                <div class="detail-panel">
                  {#if detailLoading[proposal.id]}
                    <div class="detail-state">Loading full proposal detail...</div>
                  {:else if detailErrors[proposal.id]}
                    <div class="detail-error">{detailErrors[proposal.id]}</div>
                  {:else if proposalDetails[proposal.id]}
                    <dl class="detail-meta">
                      <div><dt>Resolver</dt><dd>{proposalDetails[proposal.id].resolver ?? 'unknown'}</dd></div>
                      <div><dt>Decision</dt><dd>{proposalDetails[proposal.id].decision ?? 'pending'}</dd></div>
                    </dl>
                    <div class="detail-block">
                      <div class="detail-label">Content</div>
                      <pre class="detail-json">{formatJson(proposalDetails[proposal.id].content)}</pre>
                    </div>
                    {#if proposalDetails[proposal.id].denial_reason}
                      <div class="detail-block">
                        <div class="detail-label">Denial Reason</div>
                        <div class="detail-text">{proposalDetails[proposal.id].denial_reason}</div>
                      </div>
                    {/if}
                  {/if}
                </div>
              {/if}
            </div>

            <div class="proposal-footer">
              <button class="btn-secondary" onclick={() => toggleProposalDetail(proposal.id)}>
                {expandedProposalId === proposal.id ? 'Hide Detail' : 'View Detail'}
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .approvals-page {
    padding: var(--spacing-4);
    max-width: 960px;
    margin: 0 auto;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .page-header h1 {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
  }

  .subtitle {
    margin: var(--spacing-1) 0 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .tab-bar {
    display: flex;
    gap: var(--spacing-1);
  }

  .tab-bar button {
    padding: var(--spacing-1) var(--spacing-3);
    background: none;
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
  }

  .tab-bar button.active {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border-color: var(--color-interactive-primary);
  }

  .count-badge {
    background: var(--color-severity-hard);
    color: white;
    font-size: 10px;
    min-width: 16px;
    height: 16px;
    line-height: 16px;
    text-align: center;
    border-radius: 8px;
    padding: 0 4px;
  }

  .error-bar {
    padding: var(--spacing-2) var(--spacing-3);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    margin-bottom: var(--spacing-3);
  }

  .loading-state,
  .empty-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .proposal-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .proposal-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .proposal-card.history {
    opacity: 0.9;
  }

  .proposal-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated-2);
  }

  .agent-name {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
  }

  .proposal-type {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
  }

  .proposal-body {
    padding: var(--spacing-3);
  }

  .proposal-desc {
    margin: 0 0 var(--spacing-3);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-weight: var(--font-weight-semibold);
  }

  .proposal-meta,
  .detail-meta {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: var(--spacing-2);
    margin: 0 0 var(--spacing-3);
  }

  .proposal-meta div,
  .detail-meta div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .proposal-meta dt,
  .detail-meta dt {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
  }

  .proposal-meta dd,
  .detail-meta dd {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .mono {
    font-family: var(--font-family-mono);
  }

  .chip-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-1);
    margin-bottom: var(--spacing-3);
  }

  .chip {
    font-size: var(--font-size-xs);
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    color: var(--color-text-muted);
  }

  .scores {
    display: grid;
    gap: var(--spacing-1);
    margin-bottom: var(--spacing-3);
  }

  .score-row {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-2);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .detail-panel {
    border-top: 1px solid var(--color-border-subtle);
    margin-top: var(--spacing-3);
    padding-top: var(--spacing-3);
  }

  .detail-state,
  .detail-error,
  .detail-text {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .detail-error {
    color: var(--color-severity-hard);
  }

  .detail-block {
    margin-top: var(--spacing-3);
  }

  .detail-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
    margin-bottom: var(--spacing-1);
  }

  .detail-json {
    margin: 0;
    padding: var(--spacing-2);
    border-radius: var(--radius-sm);
    background: var(--color-bg-elevated-2);
    overflow-x: auto;
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    color: var(--color-text-primary);
  }

  .detail-list {
    margin: 0;
    padding-left: var(--spacing-4);
    display: grid;
    gap: 4px;
  }

  .history-meta {
    display: flex;
    gap: var(--spacing-3);
    flex-wrap: wrap;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .proposal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    border-top: 1px solid var(--color-border-subtle);
  }

  .action-buttons {
    display: flex;
    gap: var(--spacing-2);
  }

  .btn-approve,
  .btn-deny,
  .btn-secondary {
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
    font-weight: var(--font-weight-semibold);
  }

  .btn-approve {
    background: #22c55e;
    color: white;
    border: none;
  }

  .btn-deny {
    background: #ef4444;
    color: white;
    border: none;
  }

  .btn-secondary {
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border-default);
  }

  .status-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-weight: var(--font-weight-bold);
    text-transform: uppercase;
    margin-left: auto;
  }

  .status-pending {
    background: color-mix(in srgb, #3b82f6 15%, transparent);
    color: #3b82f6;
  }

  .status-approved {
    background: color-mix(in srgb, #22c55e 15%, transparent);
    color: #22c55e;
  }

  .status-rejected {
    background: color-mix(in srgb, #ef4444 15%, transparent);
    color: #ef4444;
  }
</style>
