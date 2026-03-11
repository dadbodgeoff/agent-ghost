<script lang="ts">
  import type { A2ATask } from '@ghost/sdk';

  interface Props {
    tasks?: A2ATask[];
    loading?: boolean;
    error?: string;
  }

  let { tasks = [], loading = false, error = '' }: Props = $props();

  function statusColor(status: string): string {
    switch (status) {
      case 'completed': return 'var(--color-score-high)';
      case 'submitted': case 'working': return 'var(--color-score-mid)';
      case 'failed': case 'canceled': return 'var(--color-severity-hard)';
      default: return 'var(--color-text-muted)';
    }
  }
</script>

<div class="tracker">
  <h3>Tracked A2A Tasks</h3>

  {#if error}
    <p class="error">{error}</p>
  {:else if loading}
    <p class="empty">Loading tasks...</p>
  {:else if tasks.length === 0}
    <p class="empty">No tracked A2A tasks yet. Send a task to an external agent to get started.</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Task ID</th>
          <th>Target Agent</th>
          <th>Method</th>
          <th>Status</th>
          <th>Created</th>
        </tr>
      </thead>
      <tbody>
        {#each tasks as task}
          <tr>
            <td class="mono">{task.task_id.slice(0, 8)}</td>
            <td>{task.target_agent}</td>
            <td class="mono">{task.method}</td>
            <td>
              <span class="status" style="color: {statusColor(task.status)}">
                {task.status}
              </span>
            </td>
            <td class="mono">{new Date(task.created_at).toLocaleString()}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  .tracker {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    margin: 0;
  }

  .empty,
  .error {
    font-size: var(--font-size-sm);
    margin: 0;
  }

  .empty {
    color: var(--color-text-muted);
  }

  .error {
    color: var(--color-severity-hard);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-xs);
  }

  th {
    text-align: left;
    padding: var(--spacing-2);
    border-bottom: 1px solid var(--color-border-default);
    color: var(--color-text-muted);
    font-weight: var(--font-weight-medium);
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  td {
    padding: var(--spacing-2);
    border-bottom: 1px solid var(--color-border-subtle);
    color: var(--color-text-secondary);
  }

  .mono {
    font-family: var(--font-family-mono);
    font-variant-numeric: tabular-nums;
  }

  .status {
    font-weight: var(--font-weight-medium);
    text-transform: capitalize;
  }
</style>
