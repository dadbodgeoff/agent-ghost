<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let goal: {
    id: string;
    description: string;
    decision: string;
    created_at: string;
    agent_id: string;
  };

  const dispatch = createEventDispatcher();

  function approve() { dispatch('approve', { id: goal.id }); }
  function reject() { dispatch('reject', { id: goal.id }); }

  $: isPending = goal.decision === 'HumanReviewRequired';
</script>

<div class="goal-card" class:pending={isPending}>
  <div class="header">
    <span class="agent">Agent: {goal.agent_id.slice(0, 8)}</span>
    <span class="decision" class:pending={isPending}>{goal.decision}</span>
  </div>
  <p class="description">{goal.description}</p>
  {#if isPending}
    <div class="actions">
      <button class="approve" on:click={approve}>Approve</button>
      <button class="reject" on:click={reject}>Reject</button>
    </div>
  {/if}
  <div class="footer">{new Date(goal.created_at).toLocaleString()}</div>
</div>

<style>
  .goal-card { background: #18181b; border: 1px solid #27272a; border-radius: 8px; padding: 12px; }
  .goal-card.pending { border-color: #854d0e; }
  .header { display: flex; justify-content: space-between; margin-bottom: 8px; font-size: 12px; }
  .agent { color: #a1a1aa; }
  .decision { font-weight: 600; color: #22c55e; }
  .decision.pending { color: #eab308; }
  .description { font-size: 13px; color: #e4e4e7; margin: 0 0 8px; }
  .actions { display: flex; gap: 8px; }
  .actions button { padding: 4px 12px; border-radius: 4px; border: none; cursor: pointer; font-size: 12px; font-weight: 600; }
  .approve { background: #166534; color: #bbf7d0; }
  .approve:hover { background: #15803d; }
  .reject { background: #991b1b; color: #fecaca; }
  .reject:hover { background: #b91c1c; }
  .footer { font-size: 11px; color: #52525b; margin-top: 8px; }
</style>
