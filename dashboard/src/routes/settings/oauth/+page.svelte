<script lang="ts">
  import { onMount } from 'svelte';
  import { getToken } from '$lib/auth';

  interface OAuthProvider {
    name: string;
    scopes: Record<string, string[]>;
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

  const API_BASE = '';

  async function fetchApi(path: string, options: RequestInit = {}) {
    const token = getToken();
    const resp = await fetch(`${API_BASE}${path}`, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${token}`,
        ...options.headers,
      },
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    return resp.json();
  }

  async function loadData() {
    try {
      loading = true;
      error = '';
      [providers, connections] = await Promise.all([
        fetchApi('/api/oauth/providers'),
        fetchApi('/api/oauth/connections'),
      ]);
    } catch (e: any) {
      error = e.message || 'Failed to load OAuth data';
    } finally {
      loading = false;
    }
  }

  async function connectProvider(name: string, scopes: string[]) {
    try {
      const data = await fetchApi('/api/oauth/connect', {
        method: 'POST',
        body: JSON.stringify({ provider: name, scopes }),
      });
      // Redirect to provider authorization page
      window.location.href = data.authorization_url;
    } catch (e: any) {
      error = `Connect failed: ${e.message}`;
    }
  }

  async function disconnectConnection(refId: string) {
    try {
      await fetchApi(`/api/oauth/connections/${refId}`, { method: 'DELETE' });
      await loadData();
    } catch (e: any) {
      error = `Disconnect failed: ${e.message}`;
    }
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'connected': return '#4caf50';
      case 'expired': return '#ff9800';
      case 'revoked': return '#f44336';
      default: return '#888';
    }
  }

  function isConnected(providerName: string): boolean {
    return connections.some(c => c.provider === providerName && c.status === 'connected');
  }

  onMount(loadData);
</script>

<h1>OAuth Connections</h1>

{#if error}
  <div class="error" role="alert">{error}</div>
{/if}

{#if loading}
  <p class="loading">Loading...</p>
{:else}
  <div class="section">
    <h2>Providers</h2>
    {#each providers as provider}
      <div class="provider-card">
        <div class="provider-header">
          <span class="provider-name">{provider.name}</span>
          {#if isConnected(provider.name)}
            <span class="badge connected">Connected</span>
          {:else}
            <button
              on:click={() => connectProvider(provider.name, Object.values(provider.scopes).flat())}
              aria-label="Connect {provider.name}"
            >
              Connect
            </button>
          {/if}
        </div>
        <div class="scopes">
          {#each Object.entries(provider.scopes) as [group, scopeList]}
            <span class="scope-group">{group}</span>
          {/each}
        </div>
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
            <span class="badge" style="background: {statusColor(conn.status)}">{conn.status}</span>
          </div>
          <div class="connection-meta">
            <span class="ref-id" title={conn.ref_id}>{conn.ref_id.slice(0, 8)}...</span>
            <span class="connected-at">{new Date(conn.connected_at).toLocaleDateString()}</span>
          </div>
          <div class="connection-scopes">
            {#each conn.scopes as scope}
              <span class="scope-tag">{scope}</span>
            {/each}
          </div>
          <button
            class="disconnect"
            on:click={() => disconnectConnection(conn.ref_id)}
            aria-label="Disconnect {conn.provider}"
          >
            Disconnect
          </button>
        </div>
      {/each}
    </div>
  {/if}
{/if}

<style>
  h1 { font-size: 20px; margin-bottom: 24px; }
  h2 { font-size: 14px; color: #888; margin-bottom: 12px; }
  .section { background: #1a1a2e; border: 1px solid #2a2a3e; border-radius: 8px; padding: 16px; margin-bottom: 16px; }
  .error { background: #3e1a1a; border: 1px solid #5e2a2a; border-radius: 4px; padding: 8px 12px; color: #ff6b6b; margin-bottom: 12px; font-size: 13px; }
  .loading { color: #888; font-size: 13px; }
  .provider-card, .connection-card { background: #12122a; border: 1px solid #2a2a3e; border-radius: 6px; padding: 12px; margin-bottom: 8px; }
  .provider-header, .connection-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
  .provider-name { font-size: 14px; font-weight: 600; color: #e0e0e0; text-transform: capitalize; }
  .badge { font-size: 11px; padding: 2px 8px; border-radius: 10px; color: #fff; }
  .badge.connected { background: #4caf50; }
  .scopes { display: flex; gap: 6px; flex-wrap: wrap; }
  .scope-group { font-size: 11px; color: #aaa; background: #2a2a3e; padding: 2px 6px; border-radius: 3px; }
  .connection-meta { display: flex; gap: 12px; font-size: 12px; color: #888; margin-bottom: 6px; }
  .ref-id { font-family: monospace; }
  .connection-scopes { display: flex; gap: 4px; flex-wrap: wrap; margin-bottom: 8px; }
  .scope-tag { font-size: 11px; color: #aaa; background: #2a2a3e; padding: 1px 5px; border-radius: 3px; }
  button { padding: 6px 14px; background: #2a2a3e; border: none; border-radius: 4px; color: #e0e0e0; cursor: pointer; font-size: 12px; }
  button:hover { background: #3a3a4e; }
  button.disconnect { background: #3e1a1a; color: #ff6b6b; }
  button.disconnect:hover { background: #5e2a2a; }
</style>
