<script lang="ts">
  /**
   * Channels Management UI (Phase 3, Task 3.1).
   * Lists all configured channel adapters with status indicators.
   * Supports add/remove/reconnect operations.
   */
  import { onMount } from 'svelte';
  import type { ChannelInfo } from '@ghost/sdk';
  import { getGhostClient } from '$lib/ghost-client';
  import { wsStore } from '$lib/stores/websocket.svelte';

  let channels: ChannelInfo[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let showAddForm = $state(false);
  let creating = $state(false);
  let actionChannelId = $state<string | null>(null);

  // Add channel form state
  let newChannelType = $state('cli');
  let newAgentId = $state('');
  let newChannelConfig = $state('{}');
  let agents: Array<{ id: string; name: string }> = $state([]);

  const CHANNEL_TYPES = ['cli', 'websocket', 'slack', 'discord', 'telegram', 'whatsapp'];
  const CHANNEL_EVENTS = [
    'ChannelCreated',
    'ChannelUpdated',
    'ChannelStatusChanged',
    'ChannelDeleted',
  ] as const;

  function statusColor(status: string): string {
    switch (status) {
      case 'connected': return 'var(--color-severity-normal)';
      case 'error': return 'var(--color-severity-hard)';
      case 'configuring': return 'var(--color-severity-soft)';
      default: return 'var(--color-text-muted)';
    }
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'connected': return 'Connected';
      case 'disconnected': return 'Disconnected';
      case 'error': return 'Error';
      case 'configuring': return 'Configuring';
      default: return status;
    }
  }

  function timeAgo(dateStr: string | null): string {
    if (!dateStr) return 'Never';
    const diff = Date.now() - new Date(dateStr).getTime();
    const mins = Math.floor(diff / 60000);
    if (mins < 1) return 'Just now';
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.floor(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }

  async function loadChannels() {
    try {
      error = '';
      const client = await getGhostClient();
      const data = await client.channels.list();
      channels = data?.channels ?? [];
      if (selectedChannel) {
        selectedChannel = channels.find((channel) => channel.id === selectedChannel?.id) ?? null;
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load channels';
    }
    loading = false;
  }

  async function loadAgents() {
    try {
      const client = await getGhostClient();
      const data = await client.agents.list();
      agents = data.map((agent) => ({ id: agent.id, name: agent.name }));
      if (agents.length > 0 && !newAgentId) {
        newAgentId = agents[0].id;
      }
    } catch { /* non-fatal */ }
  }

  function parseChannelConfig(): Record<string, unknown> | null {
    if (!newChannelConfig.trim()) {
      return {};
    }

    try {
      const parsed = JSON.parse(newChannelConfig);
      if (!parsed || Array.isArray(parsed) || typeof parsed !== 'object') {
        error = 'Channel config must be a JSON object';
        return null;
      }
      return parsed as Record<string, unknown>;
    } catch (parseError) {
      error = parseError instanceof Error ? parseError.message : 'Invalid channel config JSON';
      return null;
    }
  }

  async function addChannel() {
    if (!newAgentId) {
      error = 'Select an agent before creating a channel.';
      return;
    }
    const config = parseChannelConfig();
    if (!config) return;

    try {
      creating = true;
      error = '';
      const client = await getGhostClient();
      await client.channels.create({
        channel_type: newChannelType,
        agent_id: newAgentId,
        config,
      });
      showAddForm = false;
      newChannelConfig = '{}';
      await loadChannels();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to add channel';
    } finally {
      creating = false;
    }
  }

  async function reconnect(channelId: string) {
    try {
      actionChannelId = channelId;
      error = '';
      const client = await getGhostClient();
      await client.channels.reconnect(channelId);
      await loadChannels();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to reconnect';
    } finally {
      actionChannelId = null;
    }
  }

  async function removeChannel(channelId: string) {
    if (!confirm('Remove this channel?')) return;
    try {
      actionChannelId = channelId;
      error = '';
      const client = await getGhostClient();
      await client.channels.delete(channelId);
      if (selectedChannel?.id === channelId) {
        selectedChannel = null;
      }
      await loadChannels();
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to remove channel';
    } finally {
      actionChannelId = null;
    }
  }

  let selectedChannel: ChannelInfo | null = $state(null);

  onMount(() => {
    loadChannels();
    loadAgents();
    const unsubs = CHANNEL_EVENTS.map((eventType) => wsStore.on(eventType, () => { loadChannels(); }));
    const unsubResync = wsStore.onResync(() => { loadChannels(); });
    return () => {
      unsubs.forEach((unsubscribe) => unsubscribe());
      unsubResync();
    };
  });
</script>

<h1 class="page-title">Channels</h1>

{#if error}
  <div class="error-banner" role="alert">
    <span>{error}</span>
    <button onclick={() => (error = '')}>Dismiss</button>
  </div>
{/if}

{#if loading}
  <div class="skeleton-block">&nbsp;</div>
{:else}
  <div class="channels-toolbar">
    <span class="channel-count">{channels.length} channel{channels.length !== 1 ? 's' : ''}</span>
    <button class="btn-primary" onclick={() => (showAddForm = !showAddForm)}>
      {showAddForm ? 'Cancel' : '+ Add Channel'}
    </button>
  </div>

  {#if showAddForm}
    <div class="add-channel-form">
      <label>
        <span class="label-text">Type</span>
        <select bind:value={newChannelType}>
          {#each CHANNEL_TYPES as ct}
            <option value={ct}>{ct}</option>
          {/each}
        </select>
      </label>
      <label>
        <span class="label-text">Agent</span>
        <select bind:value={newAgentId} disabled={agents.length === 0}>
          {#each agents as agent}
            <option value={agent.id}>{agent.name}</option>
          {/each}
        </select>
      </label>
      <label class="config-field">
        <span class="label-text">Config (JSON)</span>
        <textarea bind:value={newChannelConfig} rows="5" spellcheck="false"></textarea>
      </label>
      <button class="btn-primary" onclick={addChannel} disabled={creating || agents.length === 0}>
        {creating ? 'Creating…' : 'Create'}
      </button>
      {#if agents.length === 0}
        <p class="form-hint">Create an agent first. Channels must be attached to an existing agent.</p>
      {/if}
    </div>
  {/if}

  {#if channels.length === 0}
    <div class="empty-state">
      <p>No channels configured. Add one to connect agents to external platforms.</p>
    </div>
  {:else}
    <div class="channel-list" role="list">
      {#each channels as channel (channel.id)}
        <button
          class="channel-card"
          class:selected={selectedChannel?.id === channel.id}
          onclick={() => (selectedChannel = selectedChannel?.id === channel.id ? null : channel)}
        >
          <div class="channel-main">
            <span class="status-dot" style="background: {statusColor(channel.status)}" aria-label={statusLabel(channel.status)}></span>
            <div class="channel-info">
              <span class="channel-type">{channel.channel_type}</span>
              <span class="channel-agent">{channel.agent_name ?? channel.agent_id.slice(0, 8)}</span>
            </div>
            <span class="channel-status-label" style="color: {statusColor(channel.status)}">
              {statusLabel(channel.status)}
            </span>
          </div>
          <div class="channel-meta">
            <span>Last message: {timeAgo(channel.last_message_at)}</span>
            <span>{channel.message_count} message{channel.message_count !== 1 ? 's' : ''}</span>
          </div>
          {#if channel.status === 'error' && channel.status_message}
            <div class="channel-error">{channel.status_message}</div>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  <!-- Detail panel -->
  {#if selectedChannel}
    <div class="channel-detail">
      <div class="detail-header">
        <h2>{selectedChannel.channel_type} — {selectedChannel.agent_name ?? selectedChannel.agent_id.slice(0, 8)}</h2>
        <div class="detail-actions">
          {#if selectedChannel.status === 'error' || selectedChannel.status === 'disconnected'}
            <button
              class="btn-secondary"
              onclick={() => selectedChannel && reconnect(selectedChannel.id)}
              disabled={actionChannelId === selectedChannel.id}
            >
              {actionChannelId === selectedChannel.id ? 'Reconnecting…' : 'Reconnect'}
            </button>
          {/if}
          <button
            class="btn-danger"
            onclick={() => selectedChannel && removeChannel(selectedChannel.id)}
            disabled={actionChannelId === selectedChannel.id}
          >
            {actionChannelId === selectedChannel.id ? 'Removing…' : 'Remove'}
          </button>
        </div>
      </div>
      <dl class="detail-list">
        <dt>Channel ID</dt><dd>{selectedChannel.id}</dd>
        <dt>Type</dt><dd>{selectedChannel.channel_type}</dd>
        <dt>Agent</dt><dd>{selectedChannel.agent_name ?? selectedChannel.agent_id}</dd>
        <dt>Routing Key</dt><dd class="mono">{selectedChannel.routing_key}</dd>
        <dt>Source</dt><dd>{selectedChannel.source}</dd>
        <dt>Status</dt><dd style="color: {statusColor(selectedChannel.status)}">{statusLabel(selectedChannel.status)}</dd>
        <dt>Messages</dt><dd>{selectedChannel.message_count}</dd>
        <dt>Last Message</dt><dd>{timeAgo(selectedChannel.last_message_at)}</dd>
      </dl>
      {#if Object.keys(selectedChannel.config).length > 0}
        <h3>Configuration</h3>
        <pre class="config-json">{JSON.stringify(selectedChannel.config, null, 2)}</pre>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .page-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--spacing-6);
  }

  .channels-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-4);
  }

  .channel-count {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .add-channel-form {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: var(--spacing-3);
    align-items: end;
    padding: var(--spacing-4);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
  }

  .add-channel-form label {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-1);
  }

  .label-text {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-weight: var(--font-weight-medium);
  }

  .add-channel-form select {
    min-height: 40px;
    padding: var(--spacing-2);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .config-field {
    grid-column: 1 / -1;
  }

  .config-field textarea {
    min-height: 120px;
    padding: var(--spacing-2);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    resize: vertical;
  }

  .channel-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-2);
  }

  .channel-card {
    display: block;
    width: 100%;
    text-align: left;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-3) var(--spacing-4);
    cursor: pointer;
    transition: border-color var(--duration-fast) var(--easing-default);
    color: var(--color-text-primary);
    font-family: inherit;
    font-size: inherit;
  }

  .channel-card:hover {
    border-color: var(--color-interactive-primary);
  }

  .channel-card.selected {
    border-color: var(--color-interactive-primary);
    background: var(--color-surface-selected);
  }

  .channel-main {
    display: flex;
    align-items: center;
    gap: var(--spacing-3);
    margin-bottom: var(--spacing-2);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .channel-info {
    flex: 1;
    display: flex;
    gap: var(--spacing-3);
    align-items: center;
  }

  .channel-type {
    font-weight: var(--font-weight-semibold);
    text-transform: capitalize;
  }

  .channel-agent {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .channel-status-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
  }

  .channel-meta {
    display: flex;
    gap: var(--spacing-4);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    padding-left: calc(8px + var(--spacing-3));
  }

  .channel-error {
    margin-top: var(--spacing-2);
    padding: var(--spacing-2);
    background: var(--color-severity-hard-bg, rgba(255, 0, 0, 0.1));
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    color: var(--color-severity-hard);
    padding-left: calc(8px + var(--spacing-3));
  }

  .channel-detail {
    margin-top: var(--spacing-4);
    padding: var(--spacing-4);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-4);
  }

  .detail-header h2 {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-semibold);
    text-transform: capitalize;
  }

  .detail-actions {
    display: flex;
    gap: var(--spacing-2);
  }

  .detail-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--spacing-1) var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .detail-list dt {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .detail-list dd {
    font-size: var(--font-size-sm);
    margin: 0;
  }

  .mono {
    font-family: var(--font-family-mono);
  }

  .form-hint {
    grid-column: 1 / -1;
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .channel-detail h3 {
    font-size: var(--font-size-sm);
    margin: var(--spacing-3) 0 var(--spacing-2);
  }

  .config-json {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    white-space: pre-wrap;
  }

  .btn-primary {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .btn-secondary {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .btn-danger {
    padding: var(--spacing-2) var(--spacing-4);
    background: transparent;
    color: var(--color-severity-hard);
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .error-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-severity-hard-bg, rgba(255, 0, 0, 0.1));
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-md);
    margin-bottom: var(--spacing-4);
    font-size: var(--font-size-sm);
    color: var(--color-severity-hard);
  }

  .error-banner button {
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    font-size: var(--font-size-xs);
    text-decoration: underline;
  }

  .empty-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .skeleton-block {
    height: 200px;
    background: var(--color-bg-elevated-2);
    border-radius: var(--radius-md);
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 0.7; }
  }
</style>
