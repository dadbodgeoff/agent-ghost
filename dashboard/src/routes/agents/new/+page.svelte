<script lang="ts">
  /**
   * Agent Creation Wizard — 7-step agent setup (Phase 2, Task 3.5).
   *
   * Steps: Identity → Model → System Prompt → Tools → Safety → Channels → Review
   */
  import { goto } from '$app/navigation';
  import { getGhostClient } from '$lib/ghost-client';

  interface WizardData {
    // Step 1: Identity
    name: string;
    description: string;
    icon: string;
    // Step 2: Model
    provider: string;
    model: string;
    temperature: number;
    max_tokens: number;
    // Step 3: System Prompt
    system_prompt: string;
    // Step 4: Tools
    tools: string[];
    tool_configs: Record<string, unknown>;
    // Step 5: Safety
    spending_cap: number;
    intervention_level: 'normal' | 'elevated' | 'high' | 'critical';
    convergence_profile: string;
    sandbox_enabled: boolean;
    sandbox_mode: 'off' | 'read_only' | 'workspace_write' | 'strict';
    sandbox_on_violation: 'warn' | 'pause' | 'quarantine' | 'kill_all';
    sandbox_network_access: boolean;
  // Step 6: Channels
    channels: string[];
  }

  let step = $state(1);
  let submitting = $state(false);
  let error = $state('');
  let stepErrors = $state<string[]>([]);

  let data = $state<WizardData>({
    name: '',
    description: '',
    icon: 'bot',
    provider: 'anthropic',
    model: 'claude-sonnet-4-6',
    temperature: 0.7,
    max_tokens: 4096,
    system_prompt: '',
    tools: [],
    tool_configs: {},
    spending_cap: 10,
    intervention_level: 'normal',
    convergence_profile: 'default',
    sandbox_enabled: true,
    sandbox_mode: 'workspace_write',
    sandbox_on_violation: 'pause',
    sandbox_network_access: false,
    channels: ['cli'],
  });

  const TOTAL_STEPS = 7;

  const STEP_LABELS = [
    'Identity',
    'Model',
    'System Prompt',
    'Tools',
    'Safety',
    'Channels',
    'Review',
  ];

  const AVAILABLE_PROVIDERS = [
    { value: 'anthropic', label: 'Anthropic' },
    { value: 'openai', label: 'OpenAI' },
    { value: 'google', label: 'Google' },
    { value: 'codex', label: 'Codex (ChatGPT)' },
    { value: 'ollama', label: 'Ollama (Local)' },
  ];

  const AVAILABLE_MODELS: Record<string, string[]> = {
    anthropic: ['claude-opus-4-6', 'claude-sonnet-4-6', 'claude-haiku-4-5-20251001'],
    openai: ['gpt-4o', 'gpt-4o-mini', 'o1', 'o3-mini'],
    google: ['gemini-2.0-flash', 'gemini-2.0-pro'],
    codex: ['default'],
    ollama: ['llama3.2', 'mistral', 'codellama'],
  };

  const AVAILABLE_TOOLS = [
    { id: 'shell_exec', label: 'Shell Execution', description: 'Execute shell commands' },
    { id: 'file_read', label: 'File Read', description: 'Read files from disk' },
    { id: 'file_write', label: 'File Write', description: 'Write files to disk' },
    { id: 'web_search', label: 'Web Search', description: 'Search the web' },
    { id: 'web_fetch', label: 'Web Fetch', description: 'Fetch web pages' },
    { id: 'http_request', label: 'HTTP Request', description: 'Make HTTP requests' },
    { id: 'code_analysis', label: 'Code Analysis', description: 'Analyze code structures' },
    { id: 'memory_read', label: 'Memory Read', description: 'Read from knowledge base' },
    { id: 'memory_write', label: 'Memory Write', description: 'Write to knowledge base' },
  ];

  const AVAILABLE_CHANNELS = [
    { id: 'cli', label: 'CLI', description: 'Immediate local control channel' },
    { id: 'websocket', label: 'WebSocket', description: 'Local socket adapter using the default listener config' },
  ];

  const CONVERGENCE_PROFILES = ['default', 'strict', 'permissive', 'research'];

  function validateStep(s: number): string[] {
    const errors: string[] = [];
    switch (s) {
      case 1:
        if (!data.name.trim()) errors.push('Name is required');
        if (data.name.length > 64) errors.push('Name must be 64 characters or fewer');
        if (data.name.trim() && !/^[a-z0-9-]+$/.test(data.name)) errors.push('Name: lowercase alphanumeric and hyphens only');
        break;
      case 2:
        if (!data.provider) errors.push('Select a provider');
        if (!data.model) errors.push('Select a model');
        if (data.temperature < 0 || data.temperature > 2) errors.push('Temperature must be between 0 and 2');
        if (data.max_tokens < 1 || data.max_tokens > 200000) errors.push('Max tokens must be between 1 and 200,000');
        break;
      case 5:
        if (data.spending_cap <= 0) errors.push('Spending cap must be positive');
        if (data.spending_cap > 1000) errors.push('Spending cap > $1000 requires admin approval');
        break;
    }
    return errors;
  }

  function nextStep() {
    stepErrors = validateStep(step);
    if (stepErrors.length > 0) return;
    if (step < TOTAL_STEPS) step++;
  }

  function prevStep() {
    if (step > 1) step--;
    stepErrors = [];
  }

  function toggleTool(toolId: string) {
    if (data.tools.includes(toolId)) {
      data.tools = data.tools.filter(t => t !== toolId);
    } else {
      data.tools = [...data.tools, toolId];
    }
  }

  function toggleChannel(channelId: string) {
    if (data.channels.includes(channelId)) {
      data.channels = data.channels.filter(c => c !== channelId);
    } else {
      data.channels = [...data.channels, channelId];
    }
  }

  async function submit() {
    stepErrors = validateStep(1);
    if (stepErrors.length > 0) { step = 1; return; }
    stepErrors = validateStep(2);
    if (stepErrors.length > 0) { step = 2; return; }
    stepErrors = validateStep(5);
    if (stepErrors.length > 0) { step = 5; return; }

    submitting = true;
    error = '';
    try {
      const client = await getGhostClient();
      const result = await client.agents.create({
        name: data.name,
        capabilities: data.tools,
        spending_cap: data.spending_cap,
        sandbox: {
          enabled: data.sandbox_enabled,
          mode: data.sandbox_mode,
          on_violation: data.sandbox_on_violation,
          network_access: data.sandbox_network_access,
          allowed_shell_prefixes: [],
        },
      });
      const agentId = result?.id;
      if (agentId) {
        try {
          for (const channelType of data.channels) {
            await client.channels.create({
              channel_type: channelType,
              agent_id: agentId,
            });
          }
        } catch (channelError) {
          await client.agents.delete(agentId).catch(() => undefined);
          throw new Error(
            channelError instanceof Error
              ? `Agent rollback completed after channel provisioning failed: ${channelError.message}`
              : 'Agent rollback completed after channel provisioning failed',
          );
        }
      }

      if (agentId) {
        goto(`/agents/${agentId}`);
      } else {
        goto('/agents');
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to create agent';
    } finally {
      submitting = false;
    }
  }

  let modelsForProvider = $derived(AVAILABLE_MODELS[data.provider] ?? []);
</script>

<div class="wizard-page">
  <h1>Create Agent</h1>

  <!-- Progress indicator -->
  <div class="progress-bar">
    {#each STEP_LABELS as label, i}
      <div class="progress-step" class:active={step === i + 1} class:completed={step > i + 1}>
        <div class="step-number">{i + 1}</div>
        <div class="step-label">{label}</div>
      </div>
      {#if i < STEP_LABELS.length - 1}
        <div class="step-connector" class:active={step > i + 1}></div>
      {/if}
    {/each}
  </div>

  {#if error}
    <div class="error-bar">{error}</div>
  {/if}

  {#if stepErrors.length > 0}
    <div class="validation-errors">
      {#each stepErrors as err}
        <div class="validation-error">{err}</div>
      {/each}
    </div>
  {/if}

  <div class="wizard-content">
    <!-- Step 1: Identity -->
    {#if step === 1}
      <div class="step-panel">
        <h2>Identity</h2>
        <div class="field">
          <label for="name">Name <span class="required">*</span></label>
          <input id="name" type="text" bind:value={data.name} placeholder="my-agent" />
          <span class="field-hint">Lowercase, alphanumeric, hyphens only. Max 64 chars.</span>
        </div>
        <div class="field">
          <label for="desc">Description</label>
          <textarea id="desc" bind:value={data.description} rows="3" placeholder="What does this agent do?"></textarea>
        </div>
        <div class="field">
          <label for="icon">Icon</label>
          <select id="icon" bind:value={data.icon}>
            <option value="bot">Bot</option>
            <option value="brain">Brain</option>
            <option value="code">Code</option>
            <option value="shield">Shield</option>
            <option value="search">Search</option>
            <option value="chat">Chat</option>
          </select>
        </div>
      </div>

    <!-- Step 2: Model -->
    {:else if step === 2}
      <div class="step-panel">
        <h2>Model Configuration</h2>
        <div class="field">
          <label for="provider">Provider <span class="required">*</span></label>
          <select id="provider" bind:value={data.provider} onchange={() => { data.model = modelsForProvider[0] ?? ''; }}>
            {#each AVAILABLE_PROVIDERS as p}
              <option value={p.value}>{p.label}</option>
            {/each}
          </select>
        </div>
        <div class="field">
          <label for="model">Model <span class="required">*</span></label>
          <select id="model" bind:value={data.model}>
            {#each modelsForProvider as m}
              <option value={m}>{m}</option>
            {/each}
          </select>
        </div>
        <div class="field-row">
          <div class="field">
            <label for="temp">Temperature</label>
            <input id="temp" type="number" min="0" max="2" step="0.1" bind:value={data.temperature} />
          </div>
          <div class="field">
            <label for="maxtok">Max Tokens</label>
            <input id="maxtok" type="number" min="1" max="200000" bind:value={data.max_tokens} />
          </div>
        </div>
      </div>

    <!-- Step 3: System Prompt -->
    {:else if step === 3}
      <div class="step-panel">
        <h2>System Prompt</h2>
        <div class="field">
          <label for="sysprompt">System prompt</label>
          <textarea id="sysprompt" bind:value={data.system_prompt} rows="12" placeholder="You are a helpful assistant..." class="mono-textarea"></textarea>
          <span class="field-hint">Optional. SOUL.md is automatically injected if left empty.</span>
        </div>
      </div>

    <!-- Step 4: Tools -->
    {:else if step === 4}
      <div class="step-panel">
        <h2>Tools</h2>
        <p class="step-desc">Select which tools this agent can use.</p>
        <div class="tool-grid">
          {#each AVAILABLE_TOOLS as tool}
            <label class="tool-option" class:selected={data.tools.includes(tool.id)}>
              <input type="checkbox" checked={data.tools.includes(tool.id)} onchange={() => toggleTool(tool.id)} />
              <div class="tool-info">
                <span class="tool-name">{tool.label}</span>
                <span class="tool-desc">{tool.description}</span>
              </div>
            </label>
          {/each}
        </div>
      </div>

    <!-- Step 5: Safety -->
    {:else if step === 5}
      <div class="step-panel">
        <h2>Safety Configuration</h2>
        <div class="field">
          <label for="cap">Spending Cap ($) <span class="required">*</span></label>
          <input id="cap" type="number" min="0.01" max="1000" step="0.01" bind:value={data.spending_cap} />
          <span class="field-hint">Maximum daily spend. Values over $1000 require admin approval.</span>
        </div>
        <div class="field">
          <label for="intervention">Intervention Level</label>
          <select id="intervention" bind:value={data.intervention_level}>
            <option value="normal">Normal</option>
            <option value="elevated">Elevated</option>
            <option value="high">High</option>
            <option value="critical">Critical</option>
          </select>
        </div>
        <div class="field">
          <label for="profile">Convergence Profile</label>
          <select id="profile" bind:value={data.convergence_profile}>
            {#each CONVERGENCE_PROFILES as p}
              <option value={p}>{p}</option>
            {/each}
          </select>
        </div>
        <div class="field">
          <label class="checkbox">
            <input type="checkbox" bind:checked={data.sandbox_enabled} />
            <span>Enable builtin sandbox</span>
          </label>
        </div>
        <div class="field">
          <label for="sandbox-mode">Sandbox Mode</label>
          <select id="sandbox-mode" bind:value={data.sandbox_mode}>
            <option value="off">Off</option>
            <option value="read_only">Read Only</option>
            <option value="workspace_write">Workspace Write</option>
            <option value="strict">Strict</option>
          </select>
        </div>
        <div class="field">
          <label for="sandbox-action">On Violation</label>
          <select id="sandbox-action" bind:value={data.sandbox_on_violation}>
            <option value="warn">Warn</option>
            <option value="pause">Pause</option>
            <option value="quarantine">Quarantine</option>
            <option value="kill_all">Kill All</option>
          </select>
        </div>
        <div class="field">
          <label class="checkbox">
            <input type="checkbox" bind:checked={data.sandbox_network_access} />
            <span>Allow networked builtin tools</span>
          </label>
        </div>
      </div>

    <!-- Step 6: Channels -->
    {:else if step === 6}
      <div class="step-panel">
        <h2>Channels</h2>
        <p class="step-desc">Select the bootstrap channels to create with the agent. Credentialed integrations are configured later in the dedicated Channels surface.</p>
        <div class="tool-grid">
          {#each AVAILABLE_CHANNELS as channel}
            <label class="tool-option" class:selected={data.channels.includes(channel.id)}>
              <input type="checkbox" checked={data.channels.includes(channel.id)} onchange={() => toggleChannel(channel.id)} />
              <div class="tool-info">
                <span class="tool-name">{channel.label}</span>
                <span class="tool-desc">{channel.description}</span>
              </div>
            </label>
          {/each}
        </div>
      </div>

    <!-- Step 7: Review -->
    {:else if step === 7}
      <div class="step-panel">
        <h2>Review</h2>
        <div class="review-grid">
          <div class="review-section">
            <h3>Identity</h3>
            <dl>
              <dt>Name</dt><dd>{data.name}</dd>
              <dt>Description</dt><dd>{data.description || '(none)'}</dd>
              <dt>Icon</dt><dd>{data.icon}</dd>
            </dl>
          </div>
          <div class="review-section">
            <h3>Model</h3>
            <dl>
              <dt>Provider</dt><dd>{data.provider}</dd>
              <dt>Model</dt><dd>{data.model}</dd>
              <dt>Temperature</dt><dd>{data.temperature}</dd>
              <dt>Max Tokens</dt><dd>{data.max_tokens.toLocaleString()}</dd>
            </dl>
          </div>
          <div class="review-section">
            <h3>System Prompt</h3>
            <pre class="review-code">{data.system_prompt || '(default SOUL.md)'}</pre>
          </div>
          <div class="review-section">
            <h3>Tools</h3>
            <dl><dt>Selected</dt><dd>{data.tools.length > 0 ? data.tools.join(', ') : '(none selected)'}</dd></dl>
          </div>
          <div class="review-section">
            <h3>Safety</h3>
            <dl>
              <dt>Spending Cap</dt><dd>${data.spending_cap}</dd>
              <dt>Intervention</dt><dd>{data.intervention_level}</dd>
              <dt>Convergence</dt><dd>{data.convergence_profile}</dd>
              <dt>Sandbox</dt><dd>{data.sandbox_enabled ? `${data.sandbox_mode} / ${data.sandbox_on_violation}` : 'disabled'}</dd>
            </dl>
          </div>
          <div class="review-section">
            <h3>Channels</h3>
            <dl><dt>Selected</dt><dd>{data.channels.join(', ')}</dd></dl>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <!-- Navigation -->
  <div class="wizard-nav">
    {#if step > 1}
      <button class="btn-back" onclick={prevStep}>Back</button>
    {:else}
      <div></div>
    {/if}
    {#if step < TOTAL_STEPS}
      <button class="btn-next" onclick={nextStep}>Next</button>
    {:else}
      <button class="btn-create" onclick={submit} disabled={submitting}>
        {submitting ? 'Creating...' : 'Create Agent'}
      </button>
    {/if}
  </div>
</div>

<style>
  .wizard-page {
    max-width: 700px;
    margin: 0 auto;
    padding: var(--spacing-4);
  }

  h1 {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin: 0 0 var(--spacing-4);
  }

  /* Progress bar */
  .progress-bar {
    display: flex;
    align-items: center;
    gap: 0;
    margin-bottom: var(--spacing-6);
  }

  .progress-step {
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
    flex-shrink: 0;
  }

  .step-number {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: var(--font-weight-bold);
    color: var(--color-text-muted);
  }

  .progress-step.active .step-number {
    background: var(--color-interactive-primary);
    border-color: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
  }

  .progress-step.completed .step-number {
    background: #22c55e;
    border-color: #22c55e;
    color: white;
  }

  .step-label {
    font-size: 10px;
    color: var(--color-text-muted);
    display: none;
  }

  .progress-step.active .step-label {
    display: block;
    color: var(--color-interactive-primary);
    font-weight: var(--font-weight-semibold);
  }

  .step-connector {
    flex: 1;
    height: 1px;
    background: var(--color-border-default);
    margin: 0 var(--spacing-1);
  }
  .step-connector.active { background: #22c55e; }

  /* Content */
  .wizard-content {
    min-height: 300px;
  }

  .step-panel h2 {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
    margin: 0 0 var(--spacing-4);
  }

  .step-desc {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    margin: 0 0 var(--spacing-3);
  }

  .field {
    margin-bottom: var(--spacing-4);
  }

  .field label {
    display: block;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-primary);
    margin-bottom: var(--spacing-1);
  }

  .required { color: var(--color-severity-hard); }

  .field input, .field select, .field textarea {
    width: 100%;
    padding: var(--spacing-2);
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    box-sizing: border-box;
  }
  .field input:focus, .field select:focus, .field textarea:focus {
    outline: none;
    border-color: var(--color-interactive-primary);
  }

  .mono-textarea {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
  }

  .field-hint {
    display: block;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-top: var(--spacing-1);
  }

  .field-row {
    display: flex;
    gap: var(--spacing-4);
  }
  .field-row .field { flex: 1; }

  /* Tool/Channel grid */
  .tool-grid {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .tool-option {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: border-color 0.1s;
  }
  .tool-option.selected {
    border-color: var(--color-interactive-primary);
    background: color-mix(in srgb, var(--color-interactive-primary) 5%, transparent);
  }
  .tool-option input[type="checkbox"] { flex-shrink: 0; }

  .tool-info {
    display: flex;
    flex-direction: column;
  }
  .tool-name {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
  }
  .tool-desc {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  /* Review */
  .review-grid {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .review-section {
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
  }

  .review-section h3 {
    margin: 0 0 var(--spacing-2);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  dl {
    margin: 0;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--spacing-1) var(--spacing-3);
  }
  dt {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
  dd {
    font-size: var(--font-size-xs);
    color: var(--color-text-primary);
    margin: 0;
  }

  .review-code {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-2);
    border-radius: var(--radius-sm);
    max-height: 150px;
    overflow: auto;
    white-space: pre-wrap;
    margin: 0;
    color: var(--color-text-primary);
  }

  /* Navigation */
  .wizard-nav {
    display: flex;
    justify-content: space-between;
    margin-top: var(--spacing-6);
    padding-top: var(--spacing-4);
    border-top: 1px solid var(--color-border-subtle);
  }

  .btn-back {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .btn-back:hover { background: var(--color-surface-hover); }

  .btn-next {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
  }
  .btn-next:hover { opacity: 0.9; }

  .btn-create {
    padding: var(--spacing-2) var(--spacing-6);
    background: #22c55e;
    color: white;
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
    cursor: pointer;
  }
  .btn-create:hover:not(:disabled) { opacity: 0.9; }
  .btn-create:disabled { opacity: 0.5; cursor: not-allowed; }

  .error-bar {
    padding: var(--spacing-2) var(--spacing-3);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    margin-bottom: var(--spacing-3);
  }

  .validation-errors {
    margin-bottom: var(--spacing-3);
  }
  .validation-error {
    font-size: var(--font-size-sm);
    color: var(--color-severity-hard);
    padding: var(--spacing-1) 0;
  }
</style>
