<script lang="ts">
  import type { StudioMessage } from '$lib/stores/studioChat.svelte';
  import { renderStudioMarkdown } from '$lib/render/studioMarkdown';

  interface Props {
    message: StudioMessage;
    isStreaming?: boolean;
    onRetry?: () => void;
  }
  let { message, isStreaming = false, onRetry }: Props = $props();

  let copied = $state(false);

  let renderedHtml = $derived.by(() => {
    if (message.role !== 'assistant' || !message.content) return '';
    return renderStudioMarkdown(message.content);
  });

  function formatTimestamp(iso: string): string {
    try {
      const d = new Date(iso);
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    } catch { return ''; }
  }

  async function copyContent() {
    try {
      await navigator.clipboard.writeText(message.content);
      copied = true;
      setTimeout(() => { copied = false; }, 1500);
    } catch { /* clipboard not available */ }
  }
</script>

<div
  class="chat-message"
  class:user={message.role === 'user'}
  class:assistant={message.role === 'assistant'}
  class:streaming={isStreaming}
  role="article"
  aria-label="{message.role === 'user' ? 'User' : 'Assistant'} message"
>
  <div class="msg-avatar">
    {#if message.role === 'user'}
      <div class="avatar user-avatar">U</div>
    {:else}
      <div class="avatar assistant-avatar">G</div>
    {/if}
  </div>

  <div class="msg-body">
    <div class="msg-meta">
      <span class="msg-role-label">{message.role === 'user' ? 'You' : 'GHOST'}</span>
      <span class="msg-time">{formatTimestamp(message.created_at)}</span>
      {#if message.token_count > 0}
        <span class="msg-tokens">{message.token_count} tokens</span>
      {/if}
      {#if message.safety_status && message.safety_status !== 'clean'}
        <span class="msg-safety {message.safety_status}">{message.safety_status}</span>
      {/if}
    </div>

    {#if message.role === 'user'}
      <div class="msg-text user-text">{message.content}</div>
    {:else}
      <div class="msg-text assistant-text markdown-body">
        {@html renderedHtml}
        {#if isStreaming}
          <span class="streaming-cursor"></span>
        {/if}
      </div>
    {/if}

    {#if message.toolCalls && message.toolCalls.length > 0}
      <div class="tool-calls-section">
        {#each message.toolCalls as tc (tc.toolId)}
          <div class="tool-call-entry" class:tool-running={tc.status === 'running'} class:tool-error={tc.status === 'error'} class:tool-done={tc.status === 'done'}>
            <span class="tool-icon">
              {#if tc.status === 'running'}
                <svg class="tool-spinner" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"/></svg>
              {:else if tc.status === 'error'}
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>
              {:else}
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
              {/if}
            </span>
            <span class="tool-name">{tc.tool}</span>
            {#if tc.preview && tc.status !== 'running'}
              <span class="tool-preview">{tc.preview.length > 120 ? tc.preview.slice(0, 117) + '...' : tc.preview}</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    {#if message.status === 'incomplete'}
      <div class="incomplete-banner">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/></svg>
        <span>Stream interrupted — response may be incomplete</span>
        {#if onRetry}
          <button class="incomplete-retry-btn" onclick={onRetry}>Retry</button>
        {/if}
      </div>
    {/if}

    {#if message.role === 'assistant' && !isStreaming && message.content}
      <div class="msg-actions">
        <button class="action-btn" onclick={copyContent} title={copied ? 'Copied!' : 'Copy'}>
          {#if copied}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"></polyline></svg>
          {:else}
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
          {/if}
        </button>
        {#if onRetry}
          <button class="action-btn" onclick={onRetry} title="Retry">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"></polyline><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>
          </button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .chat-message {
    display: flex;
    gap: var(--spacing-3);
    padding: var(--spacing-3) var(--spacing-4);
    max-width: 100%;
  }

  .chat-message.user { background: transparent; }

  .chat-message.assistant {
    background: var(--color-bg-elevated-1);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .msg-avatar { flex-shrink: 0; padding-top: 2px; }

  .avatar {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: var(--font-weight-bold);
  }

  .user-avatar {
    background: color-mix(in srgb, var(--color-interactive-primary) 20%, transparent);
    color: var(--color-interactive-primary);
  }

  .assistant-avatar {
    background: color-mix(in srgb, #22c55e 20%, transparent);
    color: #22c55e;
  }

  .msg-body { flex: 1; min-width: 0; overflow: hidden; }

  .msg-meta {
    display: flex;
    gap: var(--spacing-2);
    align-items: center;
    margin-bottom: var(--spacing-1);
  }

  .msg-role-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-secondary);
  }

  .msg-time { font-size: 10px; color: var(--color-text-muted); }
  .msg-tokens { font-size: 10px; color: var(--color-text-muted); font-family: var(--font-family-mono); }

  .msg-safety {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-weight: var(--font-weight-semibold);
  }
  .msg-safety.warning { background: color-mix(in srgb, orange 15%, transparent); color: orange; }
  .msg-safety.blocked { background: color-mix(in srgb, var(--color-severity-hard) 15%, transparent); color: var(--color-severity-hard); }

  .msg-text {
    font-size: var(--font-size-sm);
    line-height: 1.6;
    color: var(--color-text-primary);
    word-break: break-word;
  }

  .user-text { white-space: pre-wrap; }

  /* Markdown body styles */
  .markdown-body :global(p) { margin: 0 0 0.5em; }
  .markdown-body :global(p:last-child) { margin-bottom: 0; }

  .markdown-body :global(pre) {
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    padding: var(--spacing-3);
    overflow-x: auto;
    margin: var(--spacing-2) 0;
    font-size: var(--font-size-xs);
    line-height: 1.5;
  }

  .markdown-body :global(code) { font-family: var(--font-family-mono); font-size: 0.875em; }

  .markdown-body :global(:not(pre) > code) {
    background: color-mix(in srgb, var(--color-interactive-primary) 10%, transparent);
    padding: 1px 4px;
    border-radius: 3px;
  }

  .markdown-body :global(ul), .markdown-body :global(ol) { padding-left: 1.5em; margin: 0.5em 0; }

  .markdown-body :global(blockquote) {
    border-left: 3px solid var(--color-interactive-primary);
    padding-left: var(--spacing-3);
    margin: var(--spacing-2) 0;
    color: var(--color-text-secondary);
  }

  .markdown-body :global(table) {
    border-collapse: collapse;
    width: 100%;
    margin: var(--spacing-2) 0;
    font-size: var(--font-size-xs);
  }

  .markdown-body :global(th), .markdown-body :global(td) {
    border: 1px solid var(--color-border-subtle);
    padding: var(--spacing-1) var(--spacing-2);
    text-align: left;
  }

  .markdown-body :global(th) {
    background: var(--color-bg-elevated-1);
    font-weight: var(--font-weight-semibold);
  }

  .markdown-body :global(a) { color: var(--color-interactive-primary); text-decoration: none; }
  .markdown-body :global(a:hover) { text-decoration: underline; }

  .markdown-body :global(h1), .markdown-body :global(h2), .markdown-body :global(h3) {
    margin: 0.75em 0 0.25em;
    font-weight: var(--font-weight-semibold);
  }
  .markdown-body :global(h1) { font-size: 1.3em; }
  .markdown-body :global(h2) { font-size: 1.15em; }
  .markdown-body :global(h3) { font-size: 1.05em; }

  .markdown-body :global(hr) {
    border: none;
    border-top: 1px solid var(--color-border-subtle);
    margin: var(--spacing-3) 0;
  }

  /* Streaming cursor */
  .streaming-cursor {
    display: inline-block;
    width: 2px;
    height: 1.1em;
    background: var(--color-interactive-primary);
    margin-left: 2px;
    vertical-align: text-bottom;
    animation: blink 1s step-end infinite;
  }

  @keyframes blink {
    0%, 100% { opacity: 1; }
    50% { opacity: 0; }
  }

  /* Action buttons */
  .msg-actions {
    display: flex;
    gap: var(--spacing-1);
    margin-top: var(--spacing-2);
    opacity: 0;
    transition: opacity 0.15s;
  }

  .chat-message:hover .msg-actions { opacity: 1; }

  .action-btn {
    padding: 4px 6px;
    background: none;
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
  }

  .action-btn:hover {
    background: var(--color-bg-elevated-2);
    color: var(--color-text-primary);
    border-color: var(--color-border-default);
  }

  /* Tool call entries */
  .tool-calls-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: var(--spacing-2);
    padding: var(--spacing-2);
    background: var(--color-bg-base);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
  }

  .tool-call-entry {
    display: flex;
    align-items: flex-start;
    gap: var(--spacing-2);
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    padding: 4px 6px;
    border-radius: 3px;
  }

  .tool-call-entry.tool-running {
    color: var(--color-interactive-primary);
  }

  .tool-call-entry.tool-done {
    color: #22c55e;
  }

  .tool-call-entry.tool-error {
    color: var(--color-severity-hard);
    background: color-mix(in srgb, var(--color-severity-hard) 5%, transparent);
  }

  .tool-icon {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    padding-top: 1px;
  }

  .tool-spinner {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .tool-name {
    font-weight: var(--font-weight-semibold);
    white-space: nowrap;
  }

  .tool-preview {
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
  }

  /* Incomplete stream banner */
  .incomplete-banner {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    margin-top: var(--spacing-2);
    padding: var(--spacing-2) var(--spacing-3);
    background: color-mix(in srgb, orange 10%, transparent);
    border: 1px solid color-mix(in srgb, orange 30%, transparent);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    color: orange;
  }

  .incomplete-retry-btn {
    margin-left: auto;
    padding: 2px 10px;
    background: none;
    border: 1px solid orange;
    border-radius: var(--radius-sm);
    color: orange;
    cursor: pointer;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
  }

  .incomplete-retry-btn:hover {
    background: color-mix(in srgb, orange 15%, transparent);
  }
</style>
