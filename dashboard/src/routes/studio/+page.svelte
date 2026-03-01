<script lang="ts">
  /**
   * Prompt Playground / Agent Studio (T-2.7.1).
   * Interactive prompt testing with configurable model params.
   */
  import { api } from '$lib/api';
  import AgentTemplateSelector from '../../components/AgentTemplateSelector.svelte';

  let systemPrompt = $state('You are a helpful assistant.');
  let userMessage = $state('');
  let model = $state('claude-sonnet-4-6');
  let temperature = $state(0.5);
  let maxTokens = $state(4096);

  let running = $state(false);
  let response = $state('');
  let responseTime = $state(0);
  let tokenCount = $state(0);
  let error = $state('');

  // History
  let history: Array<{ role: string; content: string }> = $state([]);

  const MODELS = ['claude-opus-4-6', 'claude-sonnet-4-6', 'claude-haiku-4-5'] as const;

  async function runPrompt() {
    if (!userMessage.trim()) return;
    running = true;
    error = '';
    response = '';

    const messages = [
      ...history,
      { role: 'user', content: userMessage },
    ];

    const start = performance.now();
    try {
      const data = await api.post('/api/studio/run', {
        system_prompt: systemPrompt,
        messages,
        model,
        temperature,
        max_tokens: maxTokens,
      });
      responseTime = Math.round(performance.now() - start);
      response = data?.content ?? '';
      tokenCount = data?.token_count ?? 0;

      // Add to history
      history = [...history, { role: 'user', content: userMessage }, { role: 'assistant', content: response }];
      userMessage = '';
    } catch (e: any) {
      error = e.message || 'Run failed';
      responseTime = Math.round(performance.now() - start);
    }
    running = false;
  }

  function clearHistory() {
    history = [];
    response = '';
    error = '';
    tokenCount = 0;
    responseTime = 0;
  }

  function handleTemplateSelect(template: any) {
    systemPrompt = template.systemPrompt;
    model = template.model;
    temperature = template.temperature;
    maxTokens = template.maxTokens;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      runPrompt();
    }
  }
</script>

<h1 class="page-title">Studio</h1>

<!-- Template Selector -->
<details class="template-section">
  <summary class="template-toggle">Agent Templates</summary>
  <div class="template-body">
    <AgentTemplateSelector onselect={handleTemplateSelect} />
  </div>
</details>

<div class="studio-layout">
  <!-- Left: Config + Input -->
  <div class="studio-input">
    <!-- Model Config -->
    <div class="config-row">
      <label class="config-field">
        <span class="config-label">Model</span>
        <select bind:value={model} class="config-select">
          {#each MODELS as m}
            <option value={m}>{m}</option>
          {/each}
        </select>
      </label>
      <label class="config-field">
        <span class="config-label">Temp</span>
        <input type="number" bind:value={temperature} min="0" max="1" step="0.1" class="config-number" />
      </label>
      <label class="config-field">
        <span class="config-label">Max Tokens</span>
        <input type="number" bind:value={maxTokens} min="100" max="32000" step="100" class="config-number" />
      </label>
    </div>

    <!-- System Prompt -->
    <label class="field-block">
      <span class="field-label">System Prompt</span>
      <textarea bind:value={systemPrompt} rows="4" class="prompt-area"></textarea>
    </label>

    <!-- Conversation History -->
    {#if history.length > 0}
      <div class="history">
        <div class="history-header">
          <span class="history-label">Conversation ({history.length} messages)</span>
          <button class="btn-clear" onclick={clearHistory}>Clear</button>
        </div>
        <div class="history-messages">
          {#each history as msg}
            <div class="history-msg" class:user={msg.role === 'user'} class:assistant={msg.role === 'assistant'}>
              <span class="msg-role">{msg.role}</span>
              <span class="msg-content">{msg.content.length > 100 ? msg.content.slice(0, 100) + '…' : msg.content}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    <!-- User Input -->
    <label class="field-block">
      <span class="field-label">Message</span>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <textarea
        bind:value={userMessage}
        rows="4"
        class="prompt-area"
        placeholder="Type your message… (Cmd+Enter to send)"
        onkeydown={handleKeydown}
      ></textarea>
    </label>

    <button class="btn-run" disabled={running || !userMessage.trim()} onclick={runPrompt}>
      {running ? 'Running…' : 'Run'}
    </button>
  </div>

  <!-- Right: Output -->
  <div class="studio-output">
    <div class="output-header">
      <h2>Response</h2>
      {#if responseTime > 0}
        <div class="output-meta">
          <span>{responseTime}ms</span>
          {#if tokenCount > 0}
            <span>{tokenCount} tokens</span>
          {/if}
        </div>
      {/if}
    </div>

    {#if error}
      <div class="error-box">{error}</div>
    {:else if response}
      <pre class="response-text">{response}</pre>
    {:else if running}
      <div class="loading-state">Generating response…</div>
    {:else}
      <div class="empty-state">Run a prompt to see the response here.</div>
    {/if}
  </div>
</div>

<p class="sandbox-link">
  Need to simulate a full agent run? <a href="/studio/sandbox">Open Sandbox →</a>
</p>

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-4);
  }

  .template-section {
    margin-bottom: var(--spacing-4);
  }

  .template-toggle {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    cursor: pointer;
    padding: var(--spacing-2) 0;
  }

  .template-body {
    padding: var(--spacing-3) 0;
  }

  .studio-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .studio-input, .studio-output {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .config-row {
    display: flex;
    gap: var(--spacing-3);
    flex-wrap: wrap;
  }

  .config-field {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .config-label, .field-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .config-select, .config-number {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .config-number { width: 80px; }

  .field-block {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .prompt-area {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
    resize: vertical;
  }

  .prompt-area:focus {
    outline: none;
    border-color: var(--color-interactive-primary);
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
    align-self: flex-start;
  }

  .btn-run:disabled { opacity: 0.5; cursor: not-allowed; }

  .history {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    max-height: 200px;
    overflow-y: auto;
  }

  .history-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-2);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .history-label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .btn-clear {
    background: none;
    border: none;
    color: var(--color-severity-hard);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .history-messages {
    padding: var(--spacing-1) var(--spacing-2);
  }

  .history-msg {
    padding: var(--spacing-1) 0;
    font-size: var(--font-size-xs);
    display: flex;
    gap: var(--spacing-2);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .history-msg:last-child { border-bottom: none; }

  .msg-role {
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    width: 60px;
    flex-shrink: 0;
    color: var(--color-text-muted);
  }

  .msg-role:is(.user .msg-role) { color: var(--color-interactive-primary); }

  .msg-content {
    color: var(--color-text-secondary);
    word-break: break-all;
  }

  .output-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .output-header h2 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
    margin: 0;
  }

  .output-meta {
    display: flex;
    gap: var(--spacing-3);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
  }

  .response-text {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
    font-family: var(--font-family-mono);
    font-size: var(--font-size-sm);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 400px;
    overflow-y: auto;
    color: var(--color-text-primary);
  }

  .error-box {
    padding: var(--spacing-3);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
  }

  .empty-state, .loading-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .sandbox-link {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .sandbox-link a {
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .sandbox-link a:hover { text-decoration: underline; }

  @media (max-width: 768px) {
    .studio-layout { grid-template-columns: 1fr; }
  }
</style>
