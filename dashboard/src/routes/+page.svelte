<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  let score = 0;
  let level = 0;
  let agents: any[] = [];

  onMount(async () => {
    try {
      const data = await api.get('/api/convergence/scores');
      if (data) {
        score = data.composite_score || 0;
        level = data.intervention_level || 0;
      }
      agents = await api.get('/api/agents') || [];
    } catch (e) {
      console.error('Failed to load dashboard data:', e);
    }
  });
</script>

<h1>Dashboard</h1>

<div class="grid">
  <div class="card">
    <div class="card-label">Composite Score</div>
    <div class="card-value">{score.toFixed(2)}</div>
  </div>
  <div class="card">
    <div class="card-label">Intervention Level</div>
    <div class="card-value">{level}</div>
  </div>
  <div class="card">
    <div class="card-label">Active Agents</div>
    <div class="card-value">{agents.length}</div>
  </div>
</div>

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  .grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; }
  .card { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 20px; }
  .card-label { font-size: 12px; color: #888; text-transform: uppercase; letter-spacing: 1px; }
  .card-value { font-size: 36px; font-weight: 700; margin-top: 8px; }
</style>
