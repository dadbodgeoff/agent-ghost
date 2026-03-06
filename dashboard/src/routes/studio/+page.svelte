<script lang="ts">
  /**
   * Prompt Playground / Agent Studio — Enterprise chat with SSE streaming.
   */
  import { onMount } from 'svelte';
  import { studioChatStore } from '$lib/stores/studioChat.svelte';
  import ChatMessage from '../../components/ChatMessage.svelte';
  import ToolCallIndicator from '../../components/ToolCallIndicator.svelte';
  import AgentTemplateSelector from '../../components/AgentTemplateSelector.svelte';
  import 'highlight.js/styles/github-dark.css';

  let userMessage = $state('');
  let responseTime = $state(0);
  let searchQuery = $state('');
  let chatContainer: HTMLElement | null = $state(null);
  let isUserScrolledUp = $state(false);
  let selectedTemplate = $state<any>(null);

  onMount(() => {
    studioChatStore.init();
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

  // Auto-scroll on new streaming content.
  $effect(() => {
    const _ = studioChatStore.streamingContent;
    if (!isUserScrolledUp && chatContainer) {
      requestAnimationFrame(() => {
        chatContainer?.scrollTo({ top: chatContainer.scrollHeight });
      });
    }
  });

  function handleScroll() {
    if (!chatContainer) return;
    const { scrollTop, scrollHeight, clientHeight } = chatContainer;
    isUserScrolledUp = scrollHeight - scrollTop - clientHeight > 100;
  }

  async function sendMessage() {
    if (!userMessage.trim() || studioChatStore.sending) return;
    const msg = userMessage;
    userMessage = '';
    isUserScrolledUp = false;

    const start = performance.now();
    await studioChatStore.sendMessage(msg);
    responseTime = Math.round(performance.now() - start);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      sendMessage();
    }
    if (e.key === 'Escape' && studioChatStore.streaming) {
      studioChatStore.cancelStreaming();
    }
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
</script>

<div class="studio-page">
  <!-- Session Sidebar -->
  <aside class="session-sidebar">
    <div class="sidebar-header">
      <h2>Sessions</h2>
      <button class="btn-new" class:has-template={!!selectedTemplate} onclick={newChat}>
        {selectedTemplate ? `+ ${selectedTemplate.name}` : '+ New'}
      </button>
    </div>

    <div class="sidebar-search">
      <input
        type="text"
        class="search-input"
        placeholder="Search sessions..."
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
            >&times;</button>
          </li>
        {/each}
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
        <button class="btn-primary" onclick={newChat}>
          {selectedTemplate ? `Start ${selectedTemplate.name}` : 'New Chat'}
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

      <!-- Messages -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="chat-messages" bind:this={chatContainer} onscroll={handleScroll}>
        {#if messages.length === 0}
          <div class="empty-state">
            <p>Send a message to start the conversation.</p>
            {#if !session.system_prompt}
              <p class="hint">SOUL.md + environment context + 44 skills auto-injected.</p>
            {/if}
          </div>
        {:else}
          {#each messages as msg, i (msg.id || i)}
            <ChatMessage
              message={msg}
              isStreaming={studioChatStore.streaming && i === messages.length - 1 && msg.role === 'assistant'}
            />
          {/each}
        {/if}

      </div>

      <!-- Error -->
      {#if studioChatStore.error}
        <div class="error-box">{studioChatStore.error}</div>
      {/if}

      <!-- Input -->
      <div class="chat-input">
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <textarea
          bind:value={userMessage}
          rows="3"
          class="prompt-area"
          placeholder="Type your message... (Cmd+Enter to send)"
          onkeydown={handleKeydown}
          disabled={studioChatStore.sending}
        ></textarea>
        {#if studioChatStore.streaming}
          <button class="btn-stop" onclick={() => studioChatStore.cancelStreaming()}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"></rect></svg>
            Stop
          </button>
        {:else}
          <button
            class="btn-primary"
            disabled={studioChatStore.sending || !userMessage.trim()}
            onclick={sendMessage}
          >
            {studioChatStore.sending ? 'Sending...' : 'Send'}
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .studio-page {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 0;
    height: calc(100vh - 80px);
    overflow: hidden;
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
    overflow-y: auto;
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
    padding: var(--spacing-2) var(--spacing-4);
    background: color-mix(in srgb, var(--color-severity-hard) 10%, transparent);
    border-top: 1px solid var(--color-severity-hard);
    color: var(--color-severity-hard);
    font-size: var(--font-size-sm);
    flex-shrink: 0;
  }

  /* ── Input Bar ── */
  .chat-input {
    display: flex;
    gap: var(--spacing-2);
    padding: var(--spacing-3) var(--spacing-4);
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-bg-elevated-1);
    flex-shrink: 0;
  }

  .prompt-area {
    flex: 1;
    background: var(--color-bg-surface);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    padding: var(--spacing-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    font-family: var(--font-family-mono);
    resize: none;
  }

  .prompt-area:focus {
    outline: none;
    border-color: var(--color-interactive-primary);
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
  }
</style>
