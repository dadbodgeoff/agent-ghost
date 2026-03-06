<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';

  interface ProviderKeyInfo {
    provider_name: string;
    model: string;
    env_name: string;
    is_set: boolean;
    preview: string | null;
  }

  let providers = $state<ProviderKeyInfo[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);
  let editingEnv = $state<string | null>(null);
  let keyInput = $state('');
  let saving = $state(false);

  onMount(() => {
    loadProviders();
  });

  async function loadProviders() {
    loading = true;
    error = null;
    try {
      const data = await api.get('/api/admin/provider-keys');
      providers = data.providers ?? [];
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load providers';
    } finally {
      loading = false;
    }
  }

  async function saveKey(envName: string) {
    if (!keyInput.trim()) return;
    saving = true;
    error = null;
    success = null;
    try {
      const data = await api.put('/api/admin/provider-keys', {
        env_name: envName,
        value: keyInput.trim(),
      });
      success = `API key saved (${data.preview})`;
      keyInput = '';
      editingEnv = null;
      await loadProviders();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to save API key';
    } finally {
      saving = false;
    }
  }

  async function removeKey(envName: string) {
    if (!confirm(`Remove API key ${envName}? The provider will stop working until a new key is set.`)) return;
    error = null;
    success = null;
    try {
      await api.del(`/api/admin/provider-keys/${encodeURIComponent(envName)}`);
      success = 'API key removed';
      await loadProviders();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to remove API key';
    }
  }

  function startEditing(envName: string) {
    editingEnv = envName;
    keyInput = '';
    success = null;
    error = null;
  }

  function cancelEditing() {
    editingEnv = null;
    keyInput = '';
  }

  function providerLabel(name: string): string {
    const labels: Record<string, string> = {
      'anthropic': 'Anthropic (Claude)',
      'openai': 'OpenAI',
      'openai_compat': 'OpenAI-Compatible',
      'gemini': 'Google Gemini',
      'ollama': 'Ollama (Local)',
    };
    return labels[name] ?? name;
  }
</script>

<div class="page">
  <header class="page-header">
    <div>
      <h1>LLM Providers</h1>
      <p class="subtitle">Manage API keys for your configured LLM providers</p>
    </div>
  </header>

  {#if success}
    <div class="success-banner">
      <p>{success}</p>
      <button onclick={() => { success = null; }}>Dismiss</button>
    </div>
  {/if}

  {#if error}
    <div class="error-banner">
      <p>{error}</p>
      <button onclick={() => { error = null; loadProviders(); }}>Retry</button>
    </div>
  {/if}

  {#if loading}
    <p class="loading">Loading providers...</p>
  {:else if providers.length === 0}
    <div class="empty-state">
      <p>No providers configured. Add providers to <code>ghost.yml</code> to get started.</p>
    </div>
  {:else}
    <div class="provider-list">
      {#each providers as p (p.env_name || p.provider_name)}
        <div class="provider-row">
          <div class="provider-info">
            <div class="provider-header">
              <span class="provider-name">{providerLabel(p.provider_name)}</span>
              {#if p.env_name}
                <span class="provider-status" class:configured={p.is_set} class:missing={!p.is_set}>
                  {p.is_set ? 'Configured' : 'Not configured'}
                </span>
              {:else}
                <span class="provider-status configured">Local</span>
              {/if}
            </div>
            <span class="provider-model">{p.model}</span>
            {#if p.env_name}
              <span class="provider-env">{p.env_name}{p.preview ? ` = ${p.preview}` : ''}</span>
            {/if}
          </div>

          {#if p.env_name}
            <div class="provider-actions">
              {#if editingEnv === p.env_name}
                <div class="key-form">
                  <input
                    type="password"
                    class="key-input"
                    placeholder="Paste API key..."
                    bind:value={keyInput}
                    onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') saveKey(p.env_name); if (e.key === 'Escape') cancelEditing(); }}
                  />
                  <button class="save-btn" disabled={saving || !keyInput.trim()} onclick={() => saveKey(p.env_name)}>
                    {saving ? '...' : 'Save'}
                  </button>
                  <button class="cancel-btn" onclick={cancelEditing}>Cancel</button>
                </div>
              {:else}
                <button class="set-btn" onclick={() => startEditing(p.env_name)}>
                  {p.is_set ? 'Update Key' : 'Set Key'}
                </button>
                {#if p.is_set}
                  <button class="delete-btn" onclick={() => removeKey(p.env_name)}>Remove</button>
                {/if}
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    <div class="hint">
      <p>Keys are stored securely and take effect immediately. Provider configuration (name, model, base URL) is managed in <code>ghost.yml</code>.</p>
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-6);
  }

  .page-header h1 {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
    margin: 0;
  }

  .subtitle {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin: var(--spacing-1) 0 0;
  }

  .success-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-3) var(--spacing-4);
    background: color-mix(in srgb, var(--color-score-high) 10%, transparent);
    border: 1px solid var(--color-score-high);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    color: var(--color-score-high);
  }

  .success-banner p { margin: 0; }

  .success-banner button {
    background: transparent;
    border: none;
    color: var(--color-score-high);
    cursor: pointer;
    font-size: var(--font-size-xs);
  }

  .error-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-3) var(--spacing-4);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
    color: var(--color-severity-hard);
  }

  .error-banner p { margin: 0; }

  .error-banner button {
    background: transparent;
    border: 1px solid var(--color-severity-hard);
    color: var(--color-severity-hard);
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .loading {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .empty-state {
    text-align: center;
    padding: var(--spacing-8);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .empty-state code {
    font-family: var(--font-family-mono);
    background: var(--color-bg-elevated-1);
    padding: 1px var(--spacing-1);
    border-radius: var(--radius-sm);
  }

  .provider-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-3);
  }

  .provider-row {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--spacing-4);
  }

  .provider-info {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
    flex: 1;
    min-width: 0;
  }

  .provider-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
  }

  .provider-name {
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .provider-status {
    font-size: var(--font-size-xs);
    padding: 1px var(--spacing-2);
    border-radius: var(--radius-full);
  }

  .provider-status.configured {
    background: color-mix(in srgb, var(--color-score-high) 15%, transparent);
    color: var(--color-score-high);
  }

  .provider-status.missing {
    background: color-mix(in srgb, var(--color-severity-medium) 15%, transparent);
    color: var(--color-severity-medium);
  }

  .provider-model {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    font-family: var(--font-family-mono);
  }

  .provider-env {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
  }

  .provider-actions {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    flex-shrink: 0;
  }

  .key-form {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
  }

  .key-input {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-1) var(--spacing-3);
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    color: var(--color-text-primary);
    width: 220px;
  }

  .key-input:focus {
    outline: none;
    border-color: var(--color-brand-primary);
  }

  .set-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .save-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
  }

  .save-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .cancel-btn {
    background: transparent;
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border-default);
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .delete-btn {
    background: transparent;
    color: var(--color-severity-hard);
    border: 1px solid var(--color-severity-hard);
    padding: var(--spacing-1) var(--spacing-3);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .delete-btn:hover {
    background: var(--color-severity-hard);
    color: var(--color-text-inverse);
  }

  .hint {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    padding: var(--spacing-3) var(--spacing-4);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .hint p { margin: 0; }

  .hint code {
    font-family: var(--font-family-mono);
    background: var(--color-bg-elevated-2);
    padding: 1px var(--spacing-1);
    border-radius: var(--radius-sm);
  }
</style>
