<script lang="ts">
  /**
   * Proposal detail page — full validation breakdown (T-2.4.3).
   * Shows ValidationMatrix, flags, content diff, and action buttons.
   */
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { getGhostClient } from '$lib/ghost-client';
  import { GhostAPIError, type GoalDecisionRequest } from '@ghost/sdk';
  import type { ProposalDetail } from '@ghost/sdk';
  import ValidationMatrix from '../../../components/ValidationMatrix.svelte';

  let proposalId = $derived($page.params.id ?? '');
  let proposal: ProposalDetail | null = $state(null);
  let loading = $state(true);
  let error = $state('');
  let actionLoading = $state(false);

  onMount(async () => {
    try {
      const client = await getGhostClient();
      proposal = await client.goals.get(proposalId);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load proposal';
    }
    loading = false;
  });

  async function handleAction(action: 'approve' | 'reject') {
    actionLoading = true;
    try {
      const client = await getGhostClient();
      const freshDetail = await client.goals.get(proposalId);
      const request = decisionRequest(freshDetail);
      if (action === 'approve') {
        await client.goals.approve(proposalId, request);
      } else {
        await client.goals.reject(proposalId, request);
      }
      proposal = await client.goals.get(proposalId);
    } catch (e: unknown) {
      if (e instanceof GhostAPIError && e.code?.startsWith('STALE_DECISION_')) {
        error = `Failed to ${action} proposal: ${e.message}`;
      } else {
        error = e instanceof Error ? e.message : `Failed to ${action} proposal`;
      }
    }
    actionLoading = false;
  }

  let isPending = $derived.by(() => {
    return proposal?.current_state === 'pending_review';
  });

  function decisionRequest(detail: ProposalDetail): GoalDecisionRequest {
    if (
      !detail.current_state ||
      !detail.lineage_id ||
      !detail.subject_key ||
      !detail.reviewed_revision
    ) {
      throw new Error('Proposal detail is missing required lineage or revision fields');
    }

    return {
      expectedState: detail.current_state,
      expectedLineageId: detail.lineage_id,
      expectedSubjectKey: detail.subject_key,
      expectedReviewedRevision: detail.reviewed_revision,
    };
  }
</script>

{#if loading}
  <div class="loading">Loading proposal…</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <a href="/goals">← Back to Goals</a>
  </div>
{:else if proposal}
  <div class="detail-header">
    <a href="/goals" class="back-link">← Goals</a>
    <h1>Proposal {proposalId.slice(0, 8)}…</h1>
    <span class="decision-badge" class:pending={isPending} class:approved={proposal.decision === 'approved'} class:rejected={proposal.decision === 'rejected'}>
      {proposal.decision ?? 'pending'}
    </span>
  </div>

  <div class="detail-grid">
    <!-- Metadata -->
    <section class="card">
      <h2>Details</h2>
      <dl class="meta-list">
        <dt>Agent</dt><dd class="mono">{proposal.agent_id}</dd>
        <dt>Session</dt><dd class="mono">{proposal.session_id}</dd>
        <dt>Operation</dt><dd>{proposal.operation}</dd>
        <dt>Target</dt><dd>{proposal.target_type}</dd>
        <dt>Proposer</dt><dd>{proposal.proposer_type}</dd>
        <dt>Current State</dt><dd>{proposal.current_state ?? 'unknown'}</dd>
        <dt>Lineage</dt><dd class="mono">{proposal.lineage_id ?? 'unavailable'}</dd>
        <dt>Subject Key</dt><dd class="mono">{proposal.subject_key ?? 'unavailable'}</dd>
        <dt>Reviewed Revision</dt><dd class="mono">{proposal.reviewed_revision ?? 'unavailable'}</dd>
        <dt>Created</dt><dd>{new Date(proposal.created_at).toLocaleString()}</dd>
        {#if proposal.resolved_at}
          <dt>Resolved</dt><dd>{new Date(proposal.resolved_at).toLocaleString()}</dd>
          <dt>Resolver</dt><dd>{proposal.resolver ?? 'unknown'}</dd>
        {/if}
        {#if proposal.denial_reason}
          <dt>Denial Reason</dt><dd class="denial">{proposal.denial_reason}</dd>
        {/if}
      </dl>
    </section>

    <!-- Validation Matrix -->
    <section class="card">
      <h2>Validation Dimensions</h2>
      <ValidationMatrix scores={proposal.dimension_scores ?? {}} />
    </section>

    <!-- Content -->
    <section class="card wide">
      <h2>Content</h2>
      <pre class="content-json">{JSON.stringify(proposal.content, null, 2)}</pre>
    </section>

    <!-- Flags -->
    {#if proposal.flags && proposal.flags.length > 0}
      <section class="card">
        <h2>Flags</h2>
        <ul class="flag-list">
          {#each proposal.flags as flag}
            <li class="flag">{typeof flag === 'string' ? flag : JSON.stringify(flag)}</li>
          {/each}
        </ul>
      </section>
    {/if}

    <!-- Cited Memories -->
    {#if proposal.cited_memory_ids && proposal.cited_memory_ids.length > 0}
      <section class="card">
        <h2>Cited Memories</h2>
        <ul class="memory-list">
          {#each proposal.cited_memory_ids as memId}
            <li class="mono"><a href="/memory">{memId}</a></li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if proposal.transition_history && proposal.transition_history.length > 0}
      <section class="card wide">
        <h2>Transition History</h2>
        <pre class="content-json">{JSON.stringify(proposal.transition_history, null, 2)}</pre>
      </section>
    {/if}
  </div>

  <!-- Action buttons -->
  {#if isPending}
    <div class="actions">
      <button class="btn btn-approve" disabled={actionLoading} onclick={() => handleAction('approve')}>
        {actionLoading ? 'Processing…' : 'Approve'}
      </button>
      <button class="btn btn-reject" disabled={actionLoading} onclick={() => handleAction('reject')}>
        {actionLoading ? 'Processing…' : 'Reject'}
      </button>
    </div>
  {/if}
{/if}

<style>
  .loading, .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .detail-header {
    margin-bottom: var(--spacing-6);
  }

  .detail-header h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin: var(--spacing-2) 0;
  }

  .decision-badge {
    display: inline-block;
    padding: var(--spacing-0) var(--spacing-2);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-bold);
    text-transform: uppercase;
  }

  .decision-badge.pending { background: color-mix(in srgb, var(--color-severity-soft) 20%, transparent); color: var(--color-severity-soft); }
  .decision-badge.approved { background: color-mix(in srgb, var(--color-severity-normal) 20%, transparent); color: var(--color-severity-normal); }
  .decision-badge.rejected { background: color-mix(in srgb, var(--color-severity-hard) 20%, transparent); color: var(--color-severity-hard); }

  .detail-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-6);
  }

  .card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .card.wide { grid-column: 1 / -1; }

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
    word-break: break-all;
  }

  .mono { font-family: var(--font-family-mono); font-size: var(--font-size-xs); }

  .denial { color: var(--color-severity-hard); }

  .content-json {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    max-height: 300px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .flag-list { list-style: none; padding: 0; margin: 0; }

  .flag {
    padding: var(--spacing-1) var(--spacing-2);
    background: color-mix(in srgb, var(--color-severity-soft) 10%, transparent);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    margin-bottom: var(--spacing-1);
    color: var(--color-severity-soft);
  }

  .memory-list { list-style: none; padding: 0; }
  .memory-list li { margin-bottom: var(--spacing-1); }
  .memory-list a { color: var(--color-interactive-primary); text-decoration: none; }

  .actions {
    display: flex;
    gap: var(--spacing-3);
  }

  .btn {
    padding: var(--spacing-2) var(--spacing-6);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
  }

  .btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-approve { background: var(--color-severity-normal); color: var(--color-text-inverse); }
  .btn-reject { background: var(--color-severity-hard); color: var(--color-text-inverse); }
</style>
