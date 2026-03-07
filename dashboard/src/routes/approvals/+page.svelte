<script lang="ts">
  /**
   * Approval Queue — Pending agent proposals that require human approval (Phase 2, Task 3.6).
   *
   * Shows pending proposals with approve/deny/modify actions.
   * Real-time updates via WS ProposalDecision events.
   */
  import { onMount, onDestroy } from 'svelte';
  import { api } from '$lib/api';
  import { wsStore, type WsMessage } from '$lib/stores/websocket.svelte';

  interface Proposal {
    id: string;
    agent_id: string;
    agent_name: string;
    type: 'tool_call' | 'spend' | 'escalation' | 'goal_change';
    description: string;
    details: {
      tool?: string;
      args?: Record<string, unknown>;
      cost_estimate?: number;
      risk_level?: 'low' | 'medium' | 'high';
    };
    status: 'pending' | 'approved' | 'denied' | 'modified';
    created_at: string;
    decided_at?: string;
    decided_by?: string;
  }

  let proposals = $state<Proposal[]>([]);
  let loading = $state(false);
  let error = $state('');
  let activeTab = $state<'pending' | 'history'>('pending');
  let modifyingId = $state<string | null>(null);
  let modifiedArgs = $state('');
  let unsubs: Array<() => void> = [];

  let pendingProposals = $derived(proposals.filter(p => p.status === 'pending'));
  let historyProposals = $derived(proposals.filter(p => p.status !== 'pending'));

  onMount(async () => {
    await loadProposals();

    unsubs.push(
      wsStore.on('ProposalDecision', (msg: WsMessage) => {
        const data = msg as any;
        const idx = proposals.findIndex(p => p.id === data.proposal_id);
        if (idx >= 0) {
          proposals[idx] = {
            ...proposals[idx],
            status: data.decision ?? 'approved',
            decided_at: new Date().toISOString(),
            decided_by: data.decided_by ?? 'system',
          };
          proposals = [...proposals];
        }
      }),
      wsStore.on('Resync', () => {
        setTimeout(() => loadProposals(), Math.random() * 2000);
      }),
    );
  });

  onDestroy(() => {
    for (const unsub of unsubs) unsub();
  });

  async function loadProposals() {
    loading = true;
    error = '';
    try {
      const data = await api.get('/api/approvals');
      proposals = Array.isArray(data) ? data : (data as any).proposals ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load proposals';
    }
    loading = false;
  }

  async function approveProposal(id: string) {
    try {
      await api.post(`/api/approvals/${id}/approve`);
      const idx = proposals.findIndex(p => p.id === id);
      if (idx >= 0) {
        proposals[idx] = { ...proposals[idx], status: 'approved', decided_at: new Date().toISOString() };
        proposals = [...proposals];
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to approve';
    }
  }

  async function denyProposal(id: string) {
    try {
      await api.post(`/api/approvals/${id}/deny`);
      const idx = proposals.findIndex(p => p.id === id);
      if (idx >= 0) {
        proposals[idx] = { ...proposals[idx], status: 'denied', decided_at: new Date().toISOString() };
        proposals = [...proposals];
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to deny';
    }
  }

  async function modifyAndApprove(id: string) {
    try {
      const parsedArgs = JSON.parse(modifiedArgs);
      await api.post(`/api/approvals/${id}/approve`, { modified_args: parsedArgs });
      const idx = proposals.findIndex(p => p.id === id);
      if (idx >= 0) {
        proposals[idx] = { ...proposals[idx], status: 'modified', decided_at: new Date().toISOString() };
        proposals = [...proposals];
      }
      modifyingId = null;
      modifiedArgs = '';
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Invalid JSON or approval failed';
    }
  }

  function startModify(proposal: Proposal) {
    modifyingId = proposal.id;
    modifiedArgs = JSON.stringify(proposal.details.args ?? {}, null, 2);
  }

  function riskBadgeClass(risk?: string): string {
    switch (risk) {
      case 'high': return 'risk-high';
      case 'medium': return 'risk-medium';
      default: return 'risk-low';
    }
  }

  function typeIcon(type: string): string {
    switch (type) {
      case 'tool_call': return '!';
      case 'spend': return '$';
      case 'escalation': return '^';
      case 'goal_change': return '~';
      default: return '?';
    }
  }

  function relativeTime(iso: string): string {
    try {
      const diff = Date.now() - new Date(iso).getTime();
      const mins = Math.floor(diff / 60000);
      if (mins < 1) return 'just now';
      if (mins < 60) return `${mins} minutes ago`;
      const hrs = Math.floor(mins / 60);
      if (hrs < 24) return `${hrs} hours ago`;
      return `${Math.floor(hrs / 24)} days ago`;
    } catch { return ''; }
  }
</script>

<div class="approvals-page">
  <div class="page-header">
    <h1>Approvals</h1>
    <div class="tab-bar">
      <button class:active={activeTab === 'pending'} onclick={() => activeTab = 'pending'}>
        Pending {#if pendingProposals.length > 0}<span class="count-badge">{pendingProposals.length}</span>{/if}
      </button>
      <button class:active={activeTab === 'history'} onclick={() => activeTab = 'history'}>
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
      <div class="empty-state">No pending approvals</div>
    {:else}
      <div class="proposal-list">
        {#each pendingProposals as proposal (proposal.id)}
          <div class="proposal-card" class:high-risk={proposal.details.risk_level === 'high'}>
            <div class="proposal-header">
              <span class="type-icon">[{typeIcon(proposal.type)}]</span>
              <span class="agent-name">{proposal.agent_name}</span>
              <span class="proposal-type">{proposal.type.replace('_', ' ')}</span>
              {#if proposal.details.risk_level}
                <span class="risk-badge {riskBadgeClass(proposal.details.risk_level)}">
                  {proposal.details.risk_level}
                </span>
              {/if}
            </div>

            <div class="proposal-body">
              <p class="proposal-desc">{proposal.description}</p>
              {#if proposal.details.tool}
                <code class="tool-preview">{proposal.details.tool}({JSON.stringify(proposal.details.args ?? {}).slice(0, 100)})</code>
              {/if}
              {#if proposal.details.cost_estimate !== undefined}
                <span class="cost-label">Est. cost: ${proposal.details.cost_estimate.toFixed(2)}</span>
              {/if}
            </div>

            <div class="proposal-footer">
              <span class="time-label">{relativeTime(proposal.created_at)}</span>
              <div class="action-buttons">
                <button class="btn-approve" onclick={() => approveProposal(proposal.id)}>Approve</button>
                <button class="btn-deny" onclick={() => denyProposal(proposal.id)}>Deny</button>
                {#if proposal.details.args}
                  <button class="btn-modify" onclick={() => startModify(proposal)}>Modify & Approve</button>
                {/if}
              </div>
            </div>

            {#if modifyingId === proposal.id}
              <div class="modify-panel">
                <label class="modify-label">Modify arguments:</label>
                <textarea
                  class="modify-textarea"
                  bind:value={modifiedArgs}
                  rows="5"
                ></textarea>
                <div class="modify-actions">
                  <button class="btn-approve" onclick={() => modifyAndApprove(proposal.id)}>Submit Modified</button>
                  <button class="btn-cancel" onclick={() => { modifyingId = null; modifiedArgs = ''; }}>Cancel</button>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    {#if historyProposals.length === 0}
      <div class="empty-state">No decision history</div>
    {:else}
      <div class="proposal-list">
        {#each historyProposals as proposal (proposal.id)}
          <div class="proposal-card history">
            <div class="proposal-header">
              <span class="agent-name">{proposal.agent_name}</span>
              <span class="proposal-type">{proposal.type.replace('_', ' ')}</span>
              <span class="status-badge status-{proposal.status}">{proposal.status}</span>
            </div>
            <div class="proposal-body">
              <p class="proposal-desc">{proposal.description}</p>
            </div>
            <div class="proposal-footer">
              <span class="time-label">Decided {relativeTime(proposal.decided_at ?? proposal.created_at)}</span>
              {#if proposal.decided_by}
                <span class="decided-by">by {proposal.decided_by}</span>
              {/if}
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
    max-width: 900px;
    margin: 0 auto;
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-4);
  }

  .page-header h1 {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
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

  .loading-state, .empty-state {
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
  .proposal-card.high-risk {
    border-color: var(--color-severity-hard);
  }
  .proposal-card.history {
    opacity: 0.8;
  }

  .proposal-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated-2);
  }

  .type-icon {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-sm);
    color: var(--color-severity-active);
    font-weight: var(--font-weight-bold);
  }

  .agent-name {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .proposal-type {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-transform: uppercase;
  }

  .risk-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-weight: var(--font-weight-bold);
    text-transform: uppercase;
    margin-left: auto;
  }
  .risk-high { background: color-mix(in srgb, #ef4444 15%, transparent); color: #ef4444; }
  .risk-medium { background: color-mix(in srgb, #f59e0b 15%, transparent); color: #f59e0b; }
  .risk-low { background: color-mix(in srgb, #22c55e 15%, transparent); color: #22c55e; }

  .proposal-body {
    padding: var(--spacing-3);
  }

  .proposal-desc {
    margin: 0 0 var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .tool-preview {
    display: block;
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-2);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    margin-bottom: var(--spacing-2);
  }

  .cost-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
  }

  .proposal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-2) var(--spacing-3);
    border-top: 1px solid var(--color-border-subtle);
  }

  .time-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .decided-by {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .action-buttons {
    display: flex;
    gap: var(--spacing-2);
  }

  .btn-approve {
    padding: var(--spacing-1) var(--spacing-3);
    background: #22c55e;
    color: white;
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
    font-weight: var(--font-weight-semibold);
  }
  .btn-approve:hover { opacity: 0.9; }

  .btn-deny {
    padding: var(--spacing-1) var(--spacing-3);
    background: #ef4444;
    color: white;
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
    font-weight: var(--font-weight-semibold);
  }
  .btn-deny:hover { opacity: 0.9; }

  .btn-modify {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }
  .btn-modify:hover { background: var(--color-surface-hover); }

  .btn-cancel {
    padding: var(--spacing-1) var(--spacing-3);
    background: none;
    color: var(--color-text-muted);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .status-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-weight: var(--font-weight-bold);
    text-transform: uppercase;
    margin-left: auto;
  }
  .status-approved { background: color-mix(in srgb, #22c55e 15%, transparent); color: #22c55e; }
  .status-denied { background: color-mix(in srgb, #ef4444 15%, transparent); color: #ef4444; }
  .status-modified { background: color-mix(in srgb, #3b82f6 15%, transparent); color: #3b82f6; }

  .modify-panel {
    padding: var(--spacing-3);
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated-2);
  }

  .modify-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-1);
    display: block;
  }

  .modify-textarea {
    width: 100%;
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    padding: var(--spacing-2);
    resize: vertical;
    box-sizing: border-box;
  }

  .modify-actions {
    display: flex;
    gap: var(--spacing-2);
    margin-top: var(--spacing-2);
  }
</style>
