<script lang="ts">
  /**
   * Prompt Playground / Agent Studio — Enterprise chat with SSE streaming.
   */
  import { onMount, onDestroy } from 'svelte';
  import { studioChatStore } from '$lib/stores/studioChat.svelte';
  import { wsStore } from '$lib/stores/websocket.svelte';
  import { invalidateAuthClientState, notifyAuthBoundary } from '$lib/auth-boundary';
  import { getGhostClient } from '$lib/ghost-client';
  import { getRuntime, isTauriEnvironment } from '$lib/platform/runtime';
  import { shortcuts } from '$lib/shortcuts';
  import type { StudioMessage } from '$lib/stores/studioChat.svelte';
  import ChatMessage from '../../components/ChatMessage.svelte';
  import ToolCallIndicator from '../../components/ToolCallIndicator.svelte';
  import AgentTemplateSelector from '../../components/AgentTemplateSelector.svelte';
  import ArtifactPanel, { extractArtifacts, type Artifact } from '../../components/ArtifactPanel.svelte';
  import StudioInput from '../../components/StudioInput.svelte';
  import VirtualMessageList from '../../components/VirtualMessageList.svelte';
  import 'highlight.js/styles/github-dark.css';

  let responseTime = $state(0);
  let searchQuery = $state('');
  let selectedTemplate = $state<any>(null);
  let artifacts = $state<Artifact[]>([]);
  let showArtifacts = $state(false);
  let chatAreaHeight = $state(0);
  let studioInputRef = $state<StudioInput | null>(null);
  let creatingSession = $state(false);
  let resumeSyncTimer: ReturnType<typeof setTimeout> | null = null;
  let resumeSyncInFlight = false;

  // WP9-G: Auth expiry detection.
  let authExpiryWarning = $state(false);
  let authCheckInterval: ReturnType<typeof setInterval> | null = null;
  const artifactCache = new Map<string, Artifact[]>();
  const ARTIFACT_CACHE_MAX = 50;

  async function refreshAuthExpiryWarning() {
    const runtime = await getRuntime();
    const token = await runtime.getToken();

    if (!token) {
      authExpiryWarning = false;
      return;
    }

    const [, payloadSegment] = token.split('.');
    if (!payloadSegment) {
      authExpiryWarning = false;
      return;
    }

    try {
      const payload = JSON.parse(atob(payloadSegment));
      const expMs = Number(payload.exp ?? 0) * 1000;
      authExpiryWarning = Number.isFinite(expMs) && expMs > Date.now() && expMs - Date.now() < 5 * 60 * 1000;
    } catch {
      authExpiryWarning = false;
    }
  }

  function collectArtifacts(messages: StudioMessage[]): Artifact[] {
    const artifactCandidates = messages.filter(
      (msg) => msg.role === 'assistant' && !!msg.content && msg.content.includes('```'),
    );
    if (artifactCandidates.length === 0) {
      return [];
    }

    const cacheKey = artifactCandidates
      .map((msg) => `${msg.id}:${msg.content.length}`)
      .join('|');
    const cached = artifactCache.get(cacheKey);
    if (cached) {
      return cached;
    }

    const nextArtifacts = artifactCandidates.flatMap((msg) => extractArtifacts(msg.content));
    if (artifactCache.size >= ARTIFACT_CACHE_MAX) {
      const firstKey = artifactCache.keys().next().value;
      if (firstKey !== undefined) artifactCache.delete(firstKey);
    }
    artifactCache.set(cacheKey, nextArtifacts);
    return nextArtifacts;
  }

  onMount(() => {
    studioChatStore.init();
    shortcuts.registerCommand('studio.cancelStream', () => {
      studioChatStore.cancelStreaming();
    });
    let disposeTauriFocus: (() => void) | null = null;

    // WP9-G: Check JWT expiry every 60s.
    void refreshAuthExpiryWarning();
    authCheckInterval = setInterval(() => {
      void refreshAuthExpiryWarning();
    }, 60_000);

    const handleWindowFocus = () => {
      scheduleStudioResumeSync();
    };
    const handleVisibilityChange = () => {
      if (document.visibilityState === 'visible') {
        scheduleStudioResumeSync();
      }
    };

    window.addEventListener('focus', handleWindowFocus);
    window.addEventListener('pageshow', handleWindowFocus);
    document.addEventListener('visibilitychange', handleVisibilityChange);

    if (isTauriEnvironment()) {
      void import('@tauri-apps/api/window')
        .then(({ getCurrentWindow }) =>
          getCurrentWindow().onFocusChanged(({ payload }) => {
            if (payload) {
              scheduleStudioResumeSync();
            }
          }),
        )
        .then((unlisten) => {
          disposeTauriFocus = unlisten;
        })
        .catch(() => {});
    }

    return () => {
      disposeTauriFocus?.();
      window.removeEventListener('focus', handleWindowFocus);
      window.removeEventListener('pageshow', handleWindowFocus);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  });

  onDestroy(() => {
    if (resumeSyncTimer) {
      clearTimeout(resumeSyncTimer);
      resumeSyncTimer = null;
    }
    if (authCheckInterval) clearInterval(authCheckInterval);
    shortcuts.unregisterCommand('studio.cancelStream');
    shortcuts.setContext('studioStreaming', false);
  });

  $effect(() => {
    shortcuts.setContext('studioStreaming', studioChatStore.streaming);
  });

  // Derived state.
  let session = $derived(studioChatStore.activeSession);
  let messages = $derived(session?.messages ?? []);
  let filteredSessions = $derived(
    searchQuery.trim()
      ? studioChatStore.sessions.filter((s) =>
          s.title.toLowerCase().includes(searchQuery.toLowerCase())
        )
      : studioChatStore.sessions,
  );

  // Extract artifacts from messages when they change.
  $effect(() => {
    const msgs = messages;
    const allArtifacts = collectArtifacts(msgs);
    artifacts = allArtifacts;
    if (allArtifacts.length > 0) showArtifacts = true;
  });

  async function handleSend(content: string) {
    if (!content.trim() || studioChatStore.sending) return;

    const start = performance.now();
    await studioChatStore.sendMessage(content);
    responseTime = Math.round(performance.now() - start);
  }

  function handleTemplateSelect(template: any) {
    // Toggle selection — clicking the same template deselects it.
    if (selectedTemplate?.id === template.id) {
      selectedTemplate = null;
    } else {
      selectedTemplate = template;
    }
  }

  async function newChat() {
    if (creatingSession || studioChatStore.sending || studioChatStore.streaming) return;

    creatingSession = true;
    try {
      if (selectedTemplate) {
        await studioChatStore.createSession({
          title: selectedTemplate.name,
          system_prompt: selectedTemplate.systemPrompt,
          model: selectedTemplate.model,
          temperature: selectedTemplate.temperature,
          max_tokens: selectedTemplate.maxTokens,
        });
        selectedTemplate = null;
      } else {
        await studioChatStore.createSession();
      }
    } finally {
      creatingSession = false;
    }
  }

  function relativeTime(iso: string): string {
    try {
      const diff = Date.now() - new Date(iso).getTime();
      const mins = Math.floor(diff / 60000);
      if (mins < 1) return 'now';
      if (mins < 60) return `${mins}m`;
      const hrs = Math.floor(mins / 60);
      if (hrs < 24) return `${hrs}h`;
      const days = Math.floor(hrs / 24);
      return `${days}d`;
    } catch { return ''; }
  }

  function lastMessagePreview(s: typeof studioChatStore.sessions[0]): string {
    const last = s.messages?.at(-1);
    if (!last) return 'No messages';
    const text = last.content?.slice(0, 40) || '';
    return text.length >= 40 ? text + '...' : text;
  }

  async function syncStudioAfterResume() {
    if (resumeSyncInFlight) {
      return;
    }

    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') {
      return;
    }

    resumeSyncInFlight = true;
    try {
      studioInputRef?.refresh();
      await wsStore.reconnect();
      await studioChatStore.refreshActiveSession();
    } catch {
      // Best-effort foreground recovery. Existing banners surface real failures.
    } finally {
      resumeSyncInFlight = false;
    }
  }

  function scheduleStudioResumeSync() {
    if (resumeSyncTimer) {
      clearTimeout(resumeSyncTimer);
    }

    resumeSyncTimer = setTimeout(() => {
      resumeSyncTimer = null;
      void syncStudioAfterResume();
    }, 150);
  }
</script>

<div class="studio-page" class:has-artifacts={artifacts.length > 0}>
  <!-- Session Sidebar -->
  <aside class="session-sidebar">
    <div class="sidebar-header">
      <h2>Sessions</h2>
      <button
        class="btn-new"
        class:has-template={!!selectedTemplate}
        onclick={newChat}
        disabled={creatingSession || studioChatStore.sending || studioChatStore.streaming}
      >
        {#if creatingSession}
          Creating...
        {:else}
          {selectedTemplate ? `+ ${selectedTemplate.name}` : '+ New'}
        {/if}
      </button>
    </div>

    <div class="sidebar-search">
      <input
        type="text"
        class="search-input"
        placeholder="Search sessions..."
        aria-label="Search sessions"
        bind:value={searchQuery}
      />
    </div>

    {#if studioChatStore.loading}
      <div class="sidebar-loading">Loading...</div>
    {:else if filteredSessions.length === 0}
      <div class="sidebar-empty">{searchQuery ? 'No matches' : 'No sessions yet'}</div>
    {:else}
      <ul class="session-list">
        {#each filteredSessions as s (s.id)}
          <li class="session-item" class:active={s.id === studioChatStore.activeSessionId}>
            <button class="session-btn" onclick={() => studioChatStore.switchSession(s.id)}>
              <span class="session-title">{s.title}</span>
              <div class="session-bottom">
                <span class="session-preview">{lastMessagePreview(s)}</span>
                <span class="session-time">{relativeTime(s.updated_at)}</span>
              </div>
            </button>
            <button
              class="session-delete"
              onclick={(e) => { e.stopPropagation(); studioChatStore.deleteSession(s.id); }}
              title="Delete session"
              aria-label="Delete session {s.title}"
            >&times;</button>
          </li>
        {/each}
        {#if studioChatStore.hasMoreSessions && !searchQuery.trim()}
          <li class="session-load-more">
            <button class="load-more-btn" onclick={() => studioChatStore.loadMoreSessions()}>
              Load more sessions
            </button>
          </li>
        {/if}
      </ul>
    {/if}

    <details class="template-section" open={studioChatStore.sessions.length === 0 || !!selectedTemplate}>
      <summary class="template-toggle">Templates {#if selectedTemplate}<span class="template-active-dot"></span>{/if}</summary>
      <div class="template-body">
        <AgentTemplateSelector onselect={handleTemplateSelect} selectedId={selectedTemplate?.id ?? null} />
      </div>
    </details>
  </aside>

  <!-- Main Chat Area -->
  <div class="chat-area">
    <!-- Global error display — visible even without a session -->
    {#if studioChatStore.error && !session}
      <div class="error-box">{studioChatStore.error}</div>
    {/if}

    {#if !session}
      <div class="no-session">
        <div class="no-session-icon">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" opacity="0.3">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"></path>
          </svg>
        </div>
        {#if selectedTemplate}
          <p>Ready to start a <strong>{selectedTemplate.name}</strong> session.</p>
          <p class="no-session-hint">{selectedTemplate.description}</p>
        {:else}
          <p>Select a session or create a new one to start chatting.</p>
          <p class="no-session-hint">GHOST Agent with full skill access, safety pipeline, and tool execution.</p>
        {/if}
        <button class="btn-primary" onclick={newChat} disabled={creatingSession}>
          {#if creatingSession}
            Creating...
          {:else}
            {selectedTemplate ? `Start ${selectedTemplate.name}` : 'New Chat'}
          {/if}
        </button>
      </div>
    {:else}
      <!-- Session Config Bar -->
      <div class="config-bar">
        <span class="config-title">{session.title}</span>
        <div class="config-row">
          <span class="config-badge">{session.model}</span>
          <span class="config-badge">temp {session.temperature}</span>
          {#if responseTime > 0 && !studioChatStore.streaming}
            <span class="response-time">{responseTime}ms</span>
          {/if}
          {#if studioChatStore.streaming}
            <span class="streaming-badge">Streaming</span>
          {/if}
        </div>
      </div>

      <!-- WP9-G: Status banners -->
      {#if wsStore.state === 'disconnected' || wsStore.state === 'reconnecting'}
        <div class="status-banner banner-error">
          <span>Connection lost — {wsStore.state === 'reconnecting' ? `reconnecting (attempt ${wsStore.reconnectAttempt})...` : 'disconnected'}</span>
          <button class="banner-btn" onclick={() => { void wsStore.connect(); }}>Reconnect</button>
        </div>
      {/if}

      {#if authExpiryWarning}
        <div class="status-banner banner-warning">
          <span>Session expiring soon</span>
          <button class="banner-btn" onclick={async () => {
            try {
              const client = await getGhostClient();
              const data = await client.auth.refresh();
              if (data.access_token) {
                const runtime = await getRuntime();
                await runtime.setToken(data.access_token);
                invalidateAuthClientState();
                await notifyAuthBoundary('ghost-auth-session');
                authExpiryWarning = false;
              }
            } catch { /* refresh failed */ }
          }}>Refresh</button>
        </div>
      {/if}

      {#if studioChatStore.providerError}
        <div class="status-banner banner-warning">
          <span>{studioChatStore.providerError}</span>
        </div>
      {/if}

      {#if studioChatStore.persistenceWarning}
        <div class="status-banner banner-amber">
          <span>{studioChatStore.persistenceWarning}</span>
        </div>
      {/if}

      <!-- Messages (virtual scrolling for 500+ messages) -->
      <div class="chat-messages" bind:clientHeight={chatAreaHeight}>
        {#if messages.length === 0}
          <div class="empty-state">
            <p>Send a message to start the conversation.</p>
            {#if !session.system_prompt}
              <p class="hint">Default runtime prompt and environment context are injected automatically. Installed compiled skills remain subject to gateway runtime policy.</p>
            {/if}
          </div>
        {:else}
          <VirtualMessageList {messages} containerHeight={chatAreaHeight}>
            {#snippet children({ message })}
              <ChatMessage
                {message}
                isStreaming={studioChatStore.streaming && message.id === messages[messages.length - 1]?.id && message.role === 'assistant'}
              />
            {/snippet}
          </VirtualMessageList>
        {/if}
      </div>

      <!-- Error with retry -->
      {#if studioChatStore.error}
        <div class="error-box">
          <span>{studioChatStore.error}</span>
          <button class="error-retry-btn" onclick={() => studioChatStore.retryLastMessage()}>Retry</button>
        </div>
      {/if}

      <!-- Input (CodeMirror 6 with markdown highlighting) -->
      <div class="chat-input">
        <StudioInput
          bind:this={studioInputRef}
          onSend={handleSend}
          disabled={studioChatStore.sending || creatingSession}
        />
        {#if studioChatStore.streaming}
          <button class="btn-stop" onclick={() => studioChatStore.cancelStreaming()}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"></rect></svg>
            Stop
          </button>
        {:else}
            <button
              class="btn-primary"
              disabled={studioChatStore.sending || creatingSession}
              onclick={() => {
              const input = studioInputRef;
              const val = input?.getValue()?.trim();
              if (input && val) { input.clear(); handleSend(val); }
            }}
          >
            {creatingSession ? 'Creating...' : studioChatStore.sending ? 'Sending...' : 'Send'}
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Artifact Panel (Phase 2 Task 3.4) -->
  {#if artifacts.length > 0}
    <ArtifactPanel {artifacts} collapsed={!showArtifacts} />
  {/if}
</div>

<style>
  .studio-page {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 0;
    height: calc(100vh - 80px);
    overflow: hidden;
  }
  .studio-page.has-artifacts {
    grid-template-columns: 260px 1fr minmax(300px, 35%);
  }

  /* ── Sidebar ── */
  .session-sidebar {
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
    overflow-y: auto;
  }

  .sidebar-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .sidebar-header h2 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
    margin: 0;
  }

  .btn-new {
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }
  .btn-new:hover { opacity: 0.9; }
  .btn-new.has-template {
    background: color-mix(in srgb, var(--color-interactive-primary) 80%, #22c55e);
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sidebar-search {
    padding: var(--spacing-2) var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .search-input {
    width: 100%;
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    color: var(--color-text-primary);
    box-sizing: border-box;
  }
  .search-input:focus { outline: none; border-color: var(--color-interactive-primary); }

  .sidebar-loading, .sidebar-empty {
    padding: var(--spacing-4);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  .session-list {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 1;
    overflow-y: auto;
  }

  .session-item {
    display: flex;
    align-items: center;
    border-bottom: 1px solid var(--color-border-subtle);
    transition: background 0.1s;
  }

  .session-item.active {
    background: color-mix(in srgb, var(--color-interactive-primary) 10%, transparent);
    border-left: 3px solid var(--color-interactive-primary);
  }

  .session-btn {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: var(--spacing-2) var(--spacing-3);
    background: none;
    border: none;
    text-align: left;
    cursor: pointer;
    color: var(--color-text-primary);
    min-width: 0;
  }

  .session-title {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .session-bottom {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-2);
  }

  .session-preview {
    font-size: 10px;
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
  }

  .session-time {
    font-size: 10px;
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .session-delete {
    padding: var(--spacing-1) var(--spacing-2);
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: var(--font-size-sm);
    opacity: 0;
    transition: opacity 0.15s;
  }
  .session-item:hover .session-delete { opacity: 1; }
  .session-delete:hover { color: var(--color-severity-hard); }

  .session-load-more { padding: var(--spacing-2) var(--spacing-3); text-align: center; }
  .load-more-btn {
    width: 100%;
    padding: var(--spacing-1) var(--spacing-2);
    background: none;
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    color: var(--color-interactive-primary);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }
  .load-more-btn:hover { background: var(--color-bg-elevated-2); }

  .template-section {
    padding: var(--spacing-2) var(--spacing-3);
    border-top: 1px solid var(--color-border-subtle);
  }

  .template-toggle {
    font-size: var(--font-size-xs);
    color: var(--color-interactive-primary);
    cursor: pointer;
  }

  .template-body { padding: var(--spacing-2) 0; }

  .template-active-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-interactive-primary);
    margin-left: var(--spacing-1);
    vertical-align: middle;
  }

  /* ── Chat Area ── */
  .chat-area {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--color-bg-base);
  }

  .no-session {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: var(--spacing-3);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .no-session-icon { margin-bottom: var(--spacing-2); }

  .no-session-hint {
    font-size: var(--font-size-xs);
    opacity: 0.6;
    max-width: 300px;
    text-align: center;
  }

  .config-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-2) var(--spacing-4);
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated-1);
    flex-shrink: 0;
  }

  .config-title {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 250px;
  }

  .config-row {
    display: flex;
    gap: var(--spacing-2);
    align-items: center;
  }

  .config-badge {
    font-size: 10px;
    padding: 2px 8px;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
  }

  .response-time {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
  }

  .streaming-badge {
    font-size: 10px;
    padding: 2px 8px;
    background: color-mix(in srgb, #22c55e 15%, transparent);
    border: 1px solid color-mix(in srgb, #22c55e 30%, transparent);
    border-radius: var(--radius-sm);
    color: #22c55e;
    font-weight: var(--font-weight-semibold);
    animation: pulse 2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.6; }
  }

  /* ── Messages ── */
  .chat-messages {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }

  .hint { font-size: var(--font-size-xs); opacity: 0.6; margin-top: var(--spacing-1); }

  .error-box {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-4);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border-top: 1px solid var(--color-severity-hard);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    flex-shrink: 0;
  }
  .error-retry-btn {
    padding: 2px 10px;
    background: none;
    border: 1px solid var(--color-severity-hard);
    border-radius: var(--radius-sm);
    color: var(--color-severity-hard);
    cursor: pointer;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    white-space: nowrap;
  }
  .error-retry-btn:hover { background: color-mix(in srgb, var(--color-severity-hard) 15%, transparent); }

  /* WP9-G: Status banners */
  .status-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-2) var(--spacing-4);
    font-size: var(--font-size-xs);
    flex-shrink: 0;
  }
  .banner-error {
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border-bottom: 1px solid var(--color-severity-hard);
    color: var(--color-severity-hard);
  }
  .banner-warning {
    background: color-mix(in srgb, orange 10%, transparent);
    border-bottom: 1px solid color-mix(in srgb, orange 40%, transparent);
    color: orange;
  }
  .banner-amber {
    background: color-mix(in srgb, #f59e0b 10%, transparent);
    border-bottom: 1px solid color-mix(in srgb, #f59e0b 40%, transparent);
    color: #f59e0b;
  }
  .banner-btn {
    padding: 2px 10px;
    background: none;
    border: 1px solid currentColor;
    border-radius: var(--radius-sm);
    color: inherit;
    cursor: pointer;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    white-space: nowrap;
  }
  .banner-btn:hover { opacity: 0.8; }

  /* ── Input Bar ── */
  .chat-input {
    display: flex;
    gap: var(--spacing-2);
    padding: var(--spacing-3) var(--spacing-4);
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated-1);
    flex-shrink: 0;
  }

  .btn-primary {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
    align-self: flex-end;
    white-space: nowrap;
  }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-primary:hover:not(:disabled) { opacity: 0.9; }

  .btn-stop {
    padding: var(--spacing-2) var(--spacing-4);
    background: var(--color-severity-hard);
    color: white;
    border: none;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    cursor: pointer;
    align-self: flex-end;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: var(--spacing-1);
  }
  .btn-stop:hover { opacity: 0.9; }

  @media (max-width: 768px) {
    .studio-page { grid-template-columns: 1fr; }
    .session-sidebar { display: none; }
    :global(.artifact-panel) { display: none; }
  }
</style>
