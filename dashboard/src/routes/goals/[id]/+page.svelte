<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { page } from '$app/stores';
  import { getGhostClient } from '$lib/ghost-client';
  import { GhostAPIError, type ProposalDetail } from '@ghost/sdk';
  import {
    buildDecisionRequest,
    hasDecisionPrereqs,
    PENDING_PROPOSAL_STATUS,
    proposalStatus,
    statusLabel,
  } from '$lib/goal-decisions';
  import { wsStore, type WsMessage } from '$lib/stores/websocket.svelte';
  import ValidationMatrix from '../../../components/ValidationMatrix.svelte';

  let proposalId = $derived($page.params.id ?? '');
  let proposal = $state<ProposalDetail | null>(null);
  let loading = $state(true);
  let loadError = $state('');
  let actionError = $state('');
  let notice = $state('');
  let actionLoading = $state(false);
  let unsubs: Array<() => void> = [];

  let status = $derived(proposal ? proposalStatus(proposal) : 'unknown');
  let isPending = $derived(status === PENDING_PROPOSAL_STATUS);
  let decisionReady = $derived(proposal ? hasDecisionPrereqs(proposal) : false);

  onMount(() => {
    void loadProposal();

    unsubs = [
      wsStore.on('ProposalUpdated', (msg: WsMessage) => {
        const eventProposalId = (msg as { proposal_id?: string }).proposal_id;
        const supersedesProposalId = (msg as { supersedes_proposal_id?: string | null })
          .supersedes_proposal_id;
        if (eventProposalId === proposalId || supersedesProposalId === proposalId) {
          void loadProposal();
        }
      }),
      wsStore.onResync(() => {
        void loadProposal();
      }),
    ];
  });

  onDestroy(() => {
    for (const unsub of unsubs) {
      unsub();
    }
  });

  async function loadProposal() {
    loading = true;
    loadError = '';

    try {
      const client = await getGhostClient();
      proposal = await client.goals.get(proposalId);
    } catch (e: unknown) {
      loadError = e instanceof Error ? e.message : 'Failed to load proposal';
    } finally {
      loading = false;
    }
  }

  async function handleAction(action: 'approve' | 'reject') {
    actionLoading = true;
    actionError = '';
    notice = '';

    try {
      const client = await getGhostClient();
      const freshDetail = await client.goals.get(proposalId);
      const request = buildDecisionRequest(freshDetail);
      if (!request) {
        actionError =
          'Proposal is missing lineage or revision data and cannot be decided safely.';
        proposal = freshDetail;
        return;
      }
      if (action === 'approve') {
        await client.goals.approve(proposalId, request);
      } else {
        await client.goals.reject(proposalId, request);
      }
      notice = `Proposal ${proposalId.slice(0, 8)}... ${action === 'approve' ? 'approved' : 'rejected'}.`;
      await loadProposal();
    } catch (e: unknown) {
      if (e instanceof GhostAPIError && e.code?.startsWith('STALE_DECISION_')) {
        actionError = `Failed to ${action} proposal: ${e.message}`;
        await loadProposal();
      } else {
        actionError = e instanceof Error ? e.message : `Failed to ${action} proposal`;
      }
    } finally {
      actionLoading = false;
    }
  }

  function statusClass(value: string): string {
    if (value === PENDING_PROPOSAL_STATUS) return 'pending';
    if (value === 'approved') return 'approved';
    if (value === 'rejected') return 'rejected';
    return 'history';
  }
</script>

{#if loading}
  <div class="loading">Loading proposal...</div>
{:else if loadError}
  <div class="error-state">
    <p>{loadError}</p>
    <button class="btn secondary" type="button" onclick={() => void loadProposal()}>Retry</button>
    <a href="/goals">← Back to Proposals</a>
  </div>
{:else if proposal}
  <div class="detail-page">
    <div class="detail-header">
      <a href="/goals" class="back-link">← Proposals</a>
      <div class="detail-header__main">
        <div>
          <p class="eyebrow">Proposal Detail</p>
          <h1>{proposal.operation} -> {proposal.target_type}</h1>
          <p class="subtitle">Proposal {proposalId.slice(0, 8)}... for agent {proposal.agent_id}</p>
        </div>
        <span class={`decision-badge ${statusClass(status)}`}>{statusLabel(status)}</span>
      </div>
    </div>

    {#if notice}
      <div class="banner notice" role="status">{notice}</div>
    {/if}

    {#if actionError}
      <div class="banner error" role="alert">{actionError}</div>
    {/if}

    <div class="detail-grid">
      <section class="card">
        <h2>Details</h2>
        <dl class="meta-list">
          <dt>Agent</dt><dd class="mono">{proposal.agent_id}</dd>
          <dt>Session</dt><dd class="mono">{proposal.session_id}</dd>
          <dt>Operation</dt><dd>{proposal.operation}</dd>
          <dt>Target</dt><dd>{proposal.target_type}</dd>
          <dt>Proposer</dt><dd>{proposal.proposer_type}</dd>
          <dt>Canonical Status</dt><dd>{statusLabel(status)}</dd>
          <dt>Legacy Decision</dt><dd>{proposal.decision ?? 'HumanReviewRequired'}</dd>
          <dt>Lineage</dt><dd class="mono">{proposal.lineage_id ?? 'unavailable'}</dd>
          <dt>Subject Key</dt><dd class="mono">{proposal.subject_key ?? 'unavailable'}</dd>
          <dt>Reviewed Revision</dt><dd class="mono">{proposal.reviewed_revision ?? 'unavailable'}</dd>
          <dt>Created</dt><dd>{new Date(proposal.created_at).toLocaleString()}</dd>
          <dt>Resolved</dt><dd>{proposal.resolved_at ? new Date(proposal.resolved_at).toLocaleString() : 'Not yet'}</dd>
          <dt>Resolver</dt><dd>{proposal.resolver ?? 'Not yet'}</dd>
          {#if proposal.supersedes_proposal_id}
            <dt>Supersedes</dt><dd class="mono">{proposal.supersedes_proposal_id}</dd>
          {/if}
          {#if proposal.denial_reason}
            <dt>Denial Reason</dt><dd class="denial">{proposal.denial_reason}</dd>
          {/if}
        </dl>
      </section>

      <section class="card">
        <h2>Validation Dimensions</h2>
        <ValidationMatrix scores={proposal.dimension_scores ?? {}} />
        {#if !decisionReady}
          <p class="validation-warning">
            This proposal is missing canonical lineage or revision metadata, so approval controls
            stay disabled until the gateway returns a complete record.
          </p>
        {/if}
      </section>

      {#if proposal.flags.length > 0}
        <section class="card">
          <h2>Flags</h2>
          <ul class="tag-list">
            {#each proposal.flags as flag}
              <li>{flag}</li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if proposal.cited_memory_ids.length > 0}
        <section class="card">
          <h2>Cited Memories</h2>
          <ul class="tag-list mono">
            {#each proposal.cited_memory_ids as memId}
              <li><a href={`/memory/${memId}`}>{memId}</a></li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class="card wide">
        <h2>Content</h2>
        <pre class="content-json">{JSON.stringify(proposal.content, null, 2)}</pre>
      </section>

      {#if (proposal.transition_history?.length ?? 0) > 0}
        <section class="card wide">
          <h2>Transition History</h2>
          <pre class="content-json">{JSON.stringify(proposal.transition_history ?? [], null, 2)}</pre>
        </section>
      {/if}
    </div>

    {#if isPending}
      <div class="actions">
        <button class="btn secondary" disabled={actionLoading || !decisionReady} onclick={() => handleAction('reject')}>
          {actionLoading ? 'Working...' : 'Reject'}
        </button>
        <button class="btn" disabled={actionLoading || !decisionReady} onclick={() => handleAction('approve')}>
          {actionLoading ? 'Working...' : 'Approve'}
        </button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .loading,
  .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .detail-page {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-4);
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .detail-header {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .detail-header__main {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-4);
    align-items: start;
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

  .decision-badge {
    display: inline-flex;
    align-items: center;
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: 999px;
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .decision-badge.pending {
    background: color-mix(in srgb, var(--color-severity-soft) 18%, transparent);
    color: var(--color-severity-soft);
  }

  .decision-badge.approved {
    background: color-mix(in srgb, var(--color-severity-normal) 18%, transparent);
    color: var(--color-severity-normal);
  }

  .decision-badge.rejected,
  .decision-badge.history {
    background: color-mix(in srgb, var(--color-severity-hard) 12%, transparent);
    color: var(--color-text-primary);
  }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: var(--spacing-4);
  }

  .card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .card.wide {
    grid-column: 1 / -1;
  }

  .card h2 {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-3);
  }

  .meta-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--spacing-1) var(--spacing-3);
    margin: 0;
  }

  .meta-list dt {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-weight: var(--font-weight-medium);
  }

  .meta-list dd {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    margin: 0;
    word-break: break-word;
  }

  .mono {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .tag-list {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-2);
    padding: 0;
    margin: 0;
    list-style: none;
  }

  .tag-list li {
    padding: var(--spacing-1) var(--spacing-2);
    border-radius: 999px;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
  }

  .denial {
    color: var(--color-severity-hard);
  }

  .content-json {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    margin: 0;
  }

  .actions {
    display: flex;
    gap: var(--spacing-2);
    justify-content: flex-end;
  }

  .btn {
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    background: var(--color-interactive-primary);
    color: var(--color-text-inverse);
    padding: var(--spacing-2) var(--spacing-4);
    font-weight: var(--font-weight-medium);
  }

  .btn.secondary {
    background: transparent;
    color: var(--color-text-primary);
  }

  .validation-warning {
    margin: var(--spacing-3) 0 0;
    color: var(--color-severity-soft);
    font-size: var(--font-size-sm);
  }

  .btn:disabled {
    opacity: 0.65;
    cursor: wait;
  }

  @media (max-width: 720px) {
    .detail-header__main,
    .actions {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
