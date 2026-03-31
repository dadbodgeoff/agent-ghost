<script lang="ts">
  /**
   * Simulation Sandbox (T-2.7.3).
   * MVP "dry run" mode: logs what the agent would do without real tool execution.
   */
  import { getGhostClient } from '$lib/ghost-client';
  import GateCheckBar from '../../../components/GateCheckBar.svelte';

  let systemPrompt = $state('You are an AI agent. Plan tasks step-by-step.');
  let userGoal = $state('');
  let model = $state('claude-sonnet-4-6');
  let maxSteps = $state(5);

  let running = $state(false);
  let steps: Array<{
    step: number;
    action: string;
    reasoning: string;
    tool?: string;
    args?: Record<string, unknown>;
    simulated: boolean;
  }> = $state([]);
  let error = $state('');
  let completed = $state(false);

  // Simulated gate states for the sandbox run
  let gateStates = $derived(
    steps.length > 0
      ? [
          { name: 'CB', status: 'pass' as const, detail: 'Circuit Breaker' },
          { name: 'Depth', status: (steps.length >= maxSteps ? 'warning' : 'pass') as 'pass' | 'warning', detail: `${steps.length}/${maxSteps} steps` },
          { name: 'Damage', status: 'pass' as const, detail: 'Dry run — no real actions' },
          { name: 'Cap', status: 'pass' as const, detail: 'Simulated cost: $0.00' },
          { name: 'Conv', status: 'unknown' as const, detail: 'N/A in sandbox' },
          { name: 'Hash', status: 'pass' as const, detail: 'Sandbox chain valid' },
        ]
      : undefined
  );

  async function runSandbox() {
    if (!userGoal.trim()) return;
    running = true;
    steps = [];
    error = '';
    completed = false;

    try {
      const client = await getGhostClient();
      const data = await client.studio.run({
        system_prompt: systemPrompt + '\n\nIMPORTANT: This is a DRY RUN simulation. Do NOT execute any real actions. Instead, describe each step you would take, what tool you would call, and what arguments you would use. Format each step as: STEP N: [action description]. Return all steps in your response.',
        messages: [{ role: 'user', content: userGoal }],
        model,
        temperature: 0.3,
        max_tokens: 4096,
      });

      const content = data?.content ?? '';
      // Parse steps from response
      const stepRegex = /STEP\s+(\d+):\s*(.+?)(?=STEP\s+\d+:|$)/gs;
      let match;
      let stepNum = 0;
      while ((match = stepRegex.exec(content)) !== null && stepNum < maxSteps) {
        stepNum++;
        const text = match[2].trim();
        // Try to detect tool references
        const toolMatch = text.match(/(?:call|use|invoke|execute)\s+(\w+)/i);
        steps = [...steps, {
          step: stepNum,
          action: text.split('\n')[0],
          reasoning: text,
          tool: toolMatch?.[1],
          simulated: true,
        }];
      }

      if (steps.length === 0) {
        // Fallback: treat the whole response as one step
        steps = [{
          step: 1,
          action: content.slice(0, 100),
          reasoning: content,
          simulated: true,
        }];
      }

      completed = true;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Sandbox run failed';
    }
    running = false;
  }

  function reset() {
    steps = [];
    error = '';
    completed = false;
    userGoal = '';
  }
</script>

<div class="sandbox-header">
  <a href="/studio" class="back-link">← Studio</a>
  <h1>Simulation Sandbox</h1>
  <p class="subtitle">Dry-run mode — see what an agent would do without executing real actions.</p>
</div>

<div class="sandbox-config">
  <label class="field">
    <span class="field-label">Agent Goal</span>
    <textarea bind:value={userGoal} rows="3" class="goal-input" placeholder="Describe what the agent should accomplish…"></textarea>
  </label>

  <div class="config-row">
    <label class="config-field">
      <span class="config-label">Model</span>
      <select bind:value={model} class="config-select">
        <option value="claude-opus-4-6">claude-opus-4-6</option>
        <option value="claude-sonnet-4-6">claude-sonnet-4-6</option>
        <option value="claude-haiku-4-5">claude-haiku-4-5</option>
      </select>
    </label>
    <label class="config-field">
      <span class="config-label">Max Steps</span>
      <input type="number" bind:value={maxSteps} min="1" max="20" class="config-number" />
    </label>
    <div class="config-actions">
      <button class="btn-run" disabled={running || !userGoal.trim()} onclick={runSandbox}>
        {running ? 'Simulating…' : 'Run Simulation'}
      </button>
      {#if completed}
        <button class="btn-reset" onclick={reset}>Reset</button>
      {/if}
    </div>
  </div>
</div>

{#if error}
  <div class="error-box">{error}</div>
{/if}

{#if steps.length > 0}
  <!-- Gate Status -->
  <section class="gates-section">
    <GateCheckBar gates={gateStates} />
  </section>

  <!-- Steps Timeline -->
  <section class="steps-section">
    <h2>Execution Plan ({steps.length} steps)</h2>
    <ol class="steps-list">
      {#each steps as step (step.step)}
        <li class="step-item">
          <div class="step-header">
            <span class="step-num">Step {step.step}</span>
            {#if step.tool}
              <span class="step-tool">{step.tool}</span>
            {/if}
            <span class="step-badge">SIMULATED</span>
          </div>
          <p class="step-action">{step.action}</p>
          {#if step.reasoning !== step.action}
            <details class="step-reasoning">
              <summary>Full reasoning</summary>
              <pre class="reasoning-text">{step.reasoning}</pre>
            </details>
          {/if}
        </li>
      {/each}
    </ol>
  </section>
{:else if !running}
  <div class="empty-state">
    <p>Configure a goal and run the simulation to see the agent's execution plan.</p>
  </div>
{:else}
  <div class="loading-state">Simulating agent execution…</div>
{/if}

<style>
  .sandbox-header { margin-bottom: var(--spacing-4); }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .sandbox-header h1 {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin: var(--spacing-1) 0;
  }

  .subtitle {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    margin: 0;
  }

  .sandbox-config {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    margin-bottom: var(--spacing-4);
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .field, .config-field {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .field-label, .config-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .goal-input, .config-select, .config-number {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .goal-input {
    font-family: var(--font-family-mono);
    resize: vertical;
  }

  .goal-input:focus {
    outline: none;
    border-color: var(--color-interactive-primary);
  }

  .config-number { width: 80px; }

  .config-row {
    display: flex;
    gap: var(--spacing-3);
    align-items: flex-end;
    flex-wrap: wrap;
  }

  .config-actions {
    display: flex;
    gap: var(--spacing-2);
    margin-left: auto;
  }

  .btn-run {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
  }

  .btn-run:disabled { opacity: 0.5; cursor: not-allowed; }

  .btn-reset {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .error-box {
    padding: var(--spacing-3);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    margin-bottom: var(--spacing-4);
  }

  .gates-section {
    margin-bottom: var(--spacing-4);
  }

  .steps-section {
    margin-bottom: var(--spacing-6);
  }

  .steps-section h2 {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-3);
  }

  .steps-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .step-item {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
  }

  .step-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    margin-bottom: var(--spacing-2);
  }

  .step-num {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-bold);
    color: var(--color-interactive-primary);
  }

  .step-tool {
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    padding: 1px var(--spacing-1);
    background: color-mix(in srgb, var(--color-severity-active) 15%, transparent);
    color: var(--color-severity-active);
    border-radius: var(--radius-sm);
  }

  .step-badge {
    font-size: var(--font-size-xs);
    padding: 1px var(--spacing-1);
    background: color-mix(in srgb, var(--color-severity-soft) 15%, transparent);
    color: var(--color-severity-soft);
    border-radius: var(--radius-sm);
    font-weight: var(--font-weight-bold);
    margin-left: auto;
  }

  .step-action {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    margin: 0;
  }

  .step-reasoning {
    margin-top: var(--spacing-2);
  }

  .step-reasoning summary {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    cursor: pointer;
  }

  .reasoning-text {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-2);
    border-radius: var(--radius-sm);
    white-space: pre-wrap;
    word-break: break-word;
    margin-top: var(--spacing-1);
  }

  .empty-state, .loading-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
</style>
