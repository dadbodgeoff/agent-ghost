<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let goals: any[] = [];
  let loading = true;

  onMount(async () => {
    try {
      goals = await api.get('/api/goals') || [];
    } catch (e) {
      console.error('Failed to load goals:', e);
    }
    loading = false;
  });

  async function approve(id: string) {
    try {
      await api.post(`/api/goals/${id}/approve`);
      goals = goals.map(g => g.id === id ? { ...g, decision: 'approved' } : g);
    } catch (e: any) {
      if (e.message.includes('409')) {
        alert('Goal already resolved');
      }
    }
  }

  async function reject(id: string) {
    try {
      await api.post(`/api/goals/${id}/reject`);
      goals = goals.map(g => g.id === id ? { ...g, decision: 'rejected' } : g);
    } catch (e: any) {
      if (e.message.includes('409')) {
        alert('Goal already resolved');
      }
    }
  }
</script>

<h1>Goals</h1>

{#if loading}
  <p>Loading...</p>
{:else if goals.length === 0}
  <p class="empty">No pending goals</p>
{:else}
  {#each goals as goal}
    <div class="goal-card">
      <div class="goal-header">
        <span class="goal-id">{goal.id}</span>
        <span class="goal-status">{goal.decision || 'pending'}</span>
      </div>
      <p class="goal-content">{goal.content || 'No content'}</p>
      {#if !goal.decision || goal.decision === 'pending'}
        <div class="actions">
          <button class="approve" on:click={() => approve(goal.id)}>Approve</button>
          <button class="reject" on:click={() => reject(goal.id)}>Reject</button>
        </div>
      {/if}
    </div>
  {/each}
{/if}

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  .empty { color: #666; }
  .goal-card { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 16px; margin-bottom: 12px; }
  .goal-header { display: flex; justify-content: space-between; margin-bottom: 8px; }
  .goal-id { font-size: 12px; color: #666; font-family: monospace; }
  .goal-status { font-size: 12px; text-transform: uppercase; }
  .goal-content { font-size: 14px; margin-bottom: 12px; }
  .actions { display: flex; gap: 8px; }
  button { padding: 6px 16px; border: none; border-radius: 4px; cursor: pointer; font-size: 12px; }
  .approve { background: #1a3a1a; color: #4caf50; }
  .reject { background: #3a1a1a; color: #f44336; }
</style>
