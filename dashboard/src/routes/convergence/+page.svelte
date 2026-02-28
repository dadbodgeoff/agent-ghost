<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import { convergence } from '$lib/stores/convergence';

  onMount(async () => {
    try {
      const data = await api.get('/api/convergence/scores');
      if (data) {
        convergence.set({
          compositeScore: data.composite_score || 0,
          interventionLevel: data.intervention_level || 0,
          signals: data.signals || [0, 0, 0, 0, 0, 0, 0],
          lastUpdated: new Date().toISOString(),
        });
      }
    } catch (e) {
      console.error('Failed to load convergence data:', e);
    }
  });

  const signalNames = [
    'Session Duration', 'Inter-Session Gap', 'Response Latency',
    'Vocabulary Convergence', 'Goal Boundary Erosion',
    'Initiative Balance', 'Disengagement Resistance',
  ];
</script>

<h1>Convergence</h1>

<div class="score-display">
  <span class="score">{$convergence.compositeScore.toFixed(3)}</span>
  <span class="level">Level {$convergence.interventionLevel}</span>
</div>

<div class="signals">
  {#each signalNames as name, i}
    <div class="signal-row">
      <span class="name">{name}</span>
      <div class="bar-container">
        <div class="bar" style="width: {($convergence.signals[i] || 0) * 100}%"></div>
      </div>
      <span class="value">{($convergence.signals[i] || 0).toFixed(3)}</span>
    </div>
  {/each}
</div>

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  .score-display { display: flex; align-items: baseline; gap: 16px; margin-bottom: 32px; }
  .score { font-size: 48px; font-weight: 700; }
  .level { font-size: 16px; color: #888; }
  .signals { display: flex; flex-direction: column; gap: 12px; }
  .signal-row { display: flex; align-items: center; gap: 12px; }
  .name { width: 200px; font-size: 13px; color: #aaa; }
  .bar-container { flex: 1; height: 8px; background: #2a2a3e; border-radius: 4px; overflow: hidden; }
  .bar { height: 100%; background: #4040a0; border-radius: 4px; transition: width 0.3s; }
  .value { width: 60px; text-align: right; font-size: 13px; font-weight: 600; }
</style>
