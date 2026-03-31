<script lang="ts">
  /**
   * GoalCard — proposal/goal display with approve/reject actions.
   * Ref: T-1.9.4, DESIGN_SYSTEM §8.4
   */

  let {
    goal,
    onapprove,
    onreject,
  }: {
    goal: {
      id: string;
      description: string;
      decision: string;
      created_at: string;
      agent_id: string;
    };
    onapprove?: (id: string) => void;
    onreject?: (id: string) => void;
  } = $props();

  let isPending = $derived(
    !goal.decision || goal.decision === 'pending' || goal.decision === 'HumanReviewRequired'
  );
  let decisionLabel = $derived(isPending ? 'Pending' : goal.decision);
</script>

<div class="goal-card" class:pending={isPending}>
  <div class="header">
    <span class="agent">
      Agent: <code>{goal.agent_id.slice(0, 8)}</code>
    </span>
    <span class="decision" class:pending={isPending}>
      {decisionLabel}
    </span>
  </div>
  <p class="description">{goal.description}</p>
  {#if isPending}
    <div class="actions">
      <button class="approve" type="button" onclick={() => onapprove?.(goal.id)}>Approve</button>
      <button class="reject" type="button" onclick={() => onreject?.(goal.id)}>Reject</button>
    </div>
  {/if}
  <div class="footer">{new Date(goal.created_at).toLocaleString()}</div>
</div>

<style>
  .goal-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
  }

  .goal-card.pending {
    border-color: var(--color-severity-soft);
  }

  .header {
    display: flex;
    justify-content: space-between;
    margin-bottom: var(--spacing-2);
    font-size: var(--font-size-sm);
  }

  .agent {
    color: var(--color-text-secondary);
  }

  .agent code {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .decision {
    font-weight: var(--font-weight-semibold);
    color: var(--color-severity-normal);
  }

  .decision.pending {
    color: var(--color-severity-soft);
  }

  .description {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    margin: 0 0 var(--spacing-2);
    line-height: var(--line-height-normal);
  }

  .actions {
    display: flex;
    gap: var(--spacing-2);
  }

  .actions button {
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    border: none;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
    transition: background var(--duration-fast) var(--easing-default);
  }

  .actions button:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus-ring);
  }

  .approve {
    background: var(--color-severity-normal-bg);
    color: var(--color-severity-normal);
  }

  .approve:hover {
    background: var(--color-severity-normal);
    color: var(--color-text-inverse);
  }

  .reject {
    background: var(--color-severity-hard-bg);
    color: var(--color-severity-hard);
  }

  .reject:hover {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }

  .footer {
    font-size: var(--font-size-xs);
    color: var(--color-text-disabled);
    margin-top: var(--spacing-2);
  }
</style>
