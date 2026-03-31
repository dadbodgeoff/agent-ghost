<script lang="ts">
  import { onMount } from 'svelte';
  import { getGhostClient } from '$lib/ghost-client';

  interface OAuthProvider {
    name: string;
  }

  interface OAuthConnection {
    ref_id: string;
    provider: string;
    scopes: string[];
    connected_at: string;
    status: 'connected' | 'expired' | 'revoked' | 'error';
  }

  let providers: OAuthProvider[] = [];
  let connections: OAuthConnection[] = [];
  let loading = true;
  let error = '';
  let disconnectingRefId: string | null = null;
  let connectingProvider: string | null = null;

  async function loadData(showSpinner = true) {
    try {
      if (showSpinner) {
        loading = true;
      }
      error = '';
      const client = await getGhostClient();
      [providers, connections] = await Promise.all([
        client.oauth.providers(),
        client.oauth.connections(),
      ]);
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load OAuth data';
    } finally {
      loading = false;
    }
  }

  async function connectProvider(name: string, scopes: string[]) {
    try {
      error = '';
      connectingProvider = name;
      const client = await getGhostClient();
      const data = await client.oauth.connect({ provider: name, scopes });
      window.location.href = data.authorization_url;
    } catch (e: unknown) {
      error = `Connect failed: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      connectingProvider = null;
    }
  }

  async function disconnectConnection(refId: string) {
    const previousConnections = connections;
    try {
      error = '';
      disconnectingRefId = refId;
      const client = await getGhostClient();
      await client.oauth.disconnect(refId);
      connections = connections.filter((connection) => connection.ref_id !== refId);
      await loadData(false);
    } catch (e: unknown) {
      connections = previousConnections;
      error = `Disconnect failed: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      disconnectingRefId = null;
    }
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'connected': return 'var(--color-severity-normal)';
      case 'expired':   return 'var(--color-severity-soft)';
      case 'revoked':   return 'var(--color-severity-hard)';
      default:          return 'var(--color-text-muted)';
    }
  }

  function statusLabel(status: string): string {
    return status.charAt(0).toUpperCase() + status.slice(1);
  }

  function isConnected(providerName: string): boolean {
    return connections.some(c => c.provider === providerName && c.status === 'connected');
  }

  function connectionCount(providerName: string): number {
    return connections.filter((connection) => connection.provider === providerName).length;
  }

  function formatConnectedAt(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? 'Unknown date' : date.toLocaleDateString();
  }

  onMount(loadData);
</script>

<h1>OAuth Connections</h1>

{#if error}
  <div class="error" role="alert">{error}</div>
{/if}

{#if loading}
  <p class="loading">Loading...</p>
{:else if providers.length === 0 && connections.length === 0}
  <div class="empty-state">
    <p>No OAuth providers configured.</p>
    <p class="empty-hint">Add providers in your ghost.yml configuration.</p>
  </div>
{:else}
  <div class="section">
    <h2>Providers</h2>
    {#each providers as provider}
      <div class="provider-card">
        <div class="provider-header">
          <span class="provider-name">{provider.name}</span>
          {#if isConnected(provider.name)}
            <span class="badge badge-connected" aria-label="Status: connected">Connected</span>
          {:else}
            <button
              class="btn-connect"
              onclick={() => connectProvider(provider.name, [])}
              disabled={connectingProvider === provider.name}
              aria-label="Connect {provider.name}"
            >
              {connectingProvider === provider.name ? 'Connecting…' : 'Connect'}
            </button>
          {/if}
        </div>
        {#if connectionCount(provider.name) > 0}
          <p class="provider-detail">
            {connectionCount(provider.name)} saved connection{connectionCount(provider.name) === 1 ? '' : 's'}
          </p>
        {/if}
      </div>
    {/each}
  </div>

  {#if connections.length > 0}
    <div class="section">
      <h2>Active Connections</h2>
      {#each connections as conn}
        <div class="connection-card">
          <div class="connection-header">
            <span class="provider-name">{conn.provider}</span>
            <span
              class="badge"
              style="background: {statusColor(conn.status)}"
              aria-label="Status: {conn.status}"
            >
              {statusLabel(conn.status)}
            </span>
          </div>
          <div class="connection-meta">
            <span class="ref-id" title={conn.ref_id}>{conn.ref_id.slice(0, 8)}…</span>
            <span class="connected-at">{formatConnectedAt(conn.connected_at)}</span>
          </div>
          <div class="connection-scopes">
            {#if conn.scopes.length === 0}
              <span class="scope-empty">No scopes recorded</span>
            {:else}
              {#each conn.scopes as scope}
                <span class="scope-tag">{scope}</span>
              {/each}
            {/if}
          </div>
          <button
            class="btn-disconnect"
            onclick={() => disconnectConnection(conn.ref_id)}
            disabled={disconnectingRefId === conn.ref_id}
            aria-label="Disconnect {conn.provider}"
          >
            Disconnect
          </button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="section empty-state">
      <p>No active OAuth connections yet.</p>
      <p class="empty-hint">Use a provider card above to start the browser sign-in flow.</p>
    </div>
  {/if}
{/if}

<style>
  h1 {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    margin-bottom: var(--spacing-6);
  }

  h2 {
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-3);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
  }

  .section {
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .error {
    background: var(--color-severity-hard-bg);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2) var(--spacing-3);
    color: var(--color-severity-hard);
    margin-bottom: var(--spacing-3);
    font-size: var(--font-size-sm);
  }

  .loading {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .empty-state {
    text-align: center;
    padding: var(--spacing-10) var(--spacing-4);
    color: var(--color-text-secondary);
  }

  .empty-hint {
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    margin-top: var(--spacing-2);
  }

  .provider-card,
  .connection-card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3);
    margin-bottom: var(--spacing-2);
  }

  .provider-header,
  .connection-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-2);
  }

  .provider-name {
    font-size: var(--font-size-base);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    text-transform: capitalize;
  }

  .provider-detail {
    margin: 0;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .badge {
    font-size: var(--font-size-xs);
    padding: var(--spacing-0-5) var(--spacing-2);
    border-radius: var(--radius-full);
    color: var(--color-text-inverse);
    font-weight: var(--font-weight-medium);
  }

  .badge-connected {
    background: var(--color-severity-normal);
  }

  .connection-meta {
    display: flex;
    gap: var(--spacing-3);
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-1);
  }

  .ref-id {
    font-family: var(--font-family-mono);
  }

  .connection-scopes {
    display: flex;
    gap: var(--spacing-1);
    flex-wrap: wrap;
    margin-bottom: var(--spacing-2);
  }

  .scope-tag {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    background: var(--color-bg-elevated-3);
    padding: var(--spacing-0-5) var(--spacing-1);
    border-radius: var(--radius-sm);
  }

  .scope-empty {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .btn-connect {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-interactive-primary);
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-interactive-primary-text);
    cursor: pointer;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    transition: background var(--duration-fast) var(--easing-default);
  }

  .btn-connect:hover {
    background: var(--color-interactive-primary-hover);
  }

  .btn-connect:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus-ring);
  }

  .btn-disconnect {
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-severity-hard-bg);
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-severity-hard);
    cursor: pointer;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    transition: background var(--duration-fast) var(--easing-default);
  }

  .btn-disconnect:hover {
    background: var(--color-interactive-danger);
    color: var(--color-text-inverse);
  }

  .btn-disconnect:focus-visible {
    outline: none;
    box-shadow: var(--shadow-focus-ring);
  }
</style>
