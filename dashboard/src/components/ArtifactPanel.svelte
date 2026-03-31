<script lang="ts" module>
  /** Module-level exports — importable as named imports. */
  // SECURITY: artifacts must never use innerHTML without sandbox.
  // HTML artifacts are rendered inside <iframe sandbox="allow-scripts"> with DOMPurify.
  // Code/table/diff/json artifacts use safe <code> text rendering only.
  export interface Artifact {
    id: string;
    type: 'code' | 'table' | 'diff' | 'json' | 'html';
    language?: string;
    content: string;
    title?: string;
  }

  // WP5-B: Module-level memoization cache for artifact extraction.
  // Keyed by content hash (first 64 chars + length), limited to 100 entries.
  const _artifactCache = new Map<string, Artifact[]>();
  const _CACHE_MAX = 100;

  function _contentKey(content: string): string {
    // Use djb2 hash over full content to avoid prefix collisions.
    let hash = 5381;
    for (let i = 0; i < content.length; i++) {
      hash = ((hash << 5) + hash + content.charCodeAt(i)) | 0;
    }
    return `${content.length}:${(hash >>> 0).toString(36)}`;
  }

  /** Extract artifacts from chat message content.
   *  Uses deterministic IDs based on position + content hash to avoid
   *  generating new objects on every reactive re-run.
   *  Results are memoized per unique content (WP5-B). */
  export function extractArtifacts(content: string): Artifact[] {
    const key = _contentKey(content);
    const cached = _artifactCache.get(key);
    if (cached) return cached;

    const artifacts: Artifact[] = [];
    const codeBlockRegex = /```(\w+)?\n([\s\S]*?)```/g;
    let match;
    let idx = 0;
    while ((match = codeBlockRegex.exec(content)) !== null) {
      const lines = match[2].split('\n').length;
      if (lines > 5) {
        // Deterministic ID from position index + first 32 chars of content.
        const id = `artifact-${idx}-${match[2].slice(0, 32).replace(/\W/g, '_')}`;
        artifacts.push({
          id,
          type: 'code',
          language: match[1] || 'text',
          content: match[2],
          title: match[1] ? `${match[1]} snippet` : 'Code',
        });
        idx++;
      }
    }

    // Evict oldest entries when cache exceeds limit.
    if (_artifactCache.size >= _CACHE_MAX) {
      const firstKey = _artifactCache.keys().next().value;
      if (firstKey !== undefined) _artifactCache.delete(firstKey);
    }
    _artifactCache.set(key, artifacts);

    return artifacts;
  }
</script>

<script lang="ts">
  /**
   * ArtifactPanel — Side panel for code blocks, tables, and diffs (Phase 2, Task 3.4).
   *
   * When assistant returns structured output (code blocks > 5 lines), display
   * in a dedicated side panel with tabs, copy button, and language labels.
   *
   * WP9-C: HTML artifacts are rendered inside a sandboxed iframe with DOMPurify
   * sanitization to prevent XSS. Code artifacts remain as safe <code> text.
   */
  import DOMPurify from 'dompurify';

  let { artifacts = [], collapsed = false }: {
    artifacts: Artifact[];
    collapsed?: boolean;
  } = $props();

  /** WP9-C: Sanitize HTML content for safe iframe rendering.
   *  Uses ALLOWED_TAGS allowlist — any tag not listed is stripped.
   *  ALLOW_DATA_ATTR=false prevents data-* attributes from being used as
   *  attack vectors. DOMPurify strips all on* event handler attributes by
   *  default when ALLOWED_ATTR is specified (they're not in the allowlist). */
  function sanitizeHtmlArtifact(content: string): string {
    return DOMPurify.sanitize(content, {
      ALLOWED_TAGS: [
        'p', 'br', 'strong', 'em', 'code', 'pre', 'span', 'ul', 'ol', 'li',
        'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'a', 'blockquote', 'table',
        'thead', 'tbody', 'tr', 'th', 'td', 'hr', 'del', 'img', 'div',
        'details', 'summary', 'mark', 'kbd', 'sub', 'sup', 'figure', 'figcaption',
        'style', 'header', 'footer', 'nav', 'main', 'section', 'article',
        'canvas', 'svg', 'path', 'circle', 'rect', 'line', 'polyline', 'polygon',
        'text', 'g', 'defs', 'use',
      ],
      ALLOWED_ATTR: [
        'class', 'href', 'target', 'rel', 'src', 'alt', 'title', 'style',
        'width', 'height', 'viewBox', 'xmlns', 'd', 'fill', 'stroke',
        'stroke-width', 'cx', 'cy', 'r', 'x', 'y', 'x1', 'y1', 'x2', 'y2',
        'points', 'transform', 'id',
      ],
      ALLOW_DATA_ATTR: false,
    });
  }

  let activeArtifactId = $state<string | undefined>(undefined);
  let copyFeedback = $state<string | null>(null);
  let copyFeedbackTimer: ReturnType<typeof setTimeout> | null = null;

  let activeArtifact = $derived(
    artifacts.find(a => a.id === activeArtifactId) ?? artifacts[0]
  );

  $effect(() => {
    if (artifacts.length === 0) {
      activeArtifactId = undefined;
      return;
    }

    if (!artifacts.some((artifact) => artifact.id === activeArtifactId)) {
      activeArtifactId = artifacts[0]?.id;
    }
  });

  async function copyToClipboard(content: string) {
    if (copyFeedbackTimer) {
      clearTimeout(copyFeedbackTimer);
    }
    try {
      await navigator.clipboard.writeText(content);
      copyFeedback = 'Copied!';
    } catch {
      copyFeedback = 'Failed to copy';
    }
    copyFeedbackTimer = setTimeout(() => {
      copyFeedback = null;
      copyFeedbackTimer = null;
    }, 2000);
  }

  function renderDiffLine(line: string): { class: string; text: string } {
    if (line.startsWith('+')) return { class: 'diff-add', text: line };
    if (line.startsWith('-')) return { class: 'diff-remove', text: line };
    if (line.startsWith('@')) return { class: 'diff-hunk', text: line };
    return { class: 'diff-context', text: line };
  }
</script>

{#if artifacts.length > 0 && !collapsed}
<div class="artifact-panel" role="complementary" aria-label="Artifacts">
  <!-- Tab bar for multiple artifacts -->
  <div class="artifact-tabs" role="tablist">
    {#each artifacts as artifact}
      <button
        role="tab"
        aria-selected={artifact.id === activeArtifact?.id}
        aria-controls={`artifact-panel-${artifact.id}`}
        class:active={artifact.id === activeArtifact?.id}
        onclick={() => activeArtifactId = artifact.id}
      >
        {artifact.title ?? artifact.type}
      </button>
    {/each}
  </div>

  <!-- Content area -->
  <div class="artifact-content" role="tabpanel" id={activeArtifact ? `artifact-panel-${activeArtifact.id}` : undefined}>
    {#if activeArtifact?.type === 'code'}
      <div class="artifact-code">
        <div class="artifact-toolbar">
          <span class="artifact-language">{activeArtifact.language}</span>
          <button class="copy-btn" onclick={() => copyToClipboard(activeArtifact.content)}>
            {copyFeedback ?? 'Copy'}
          </button>
        </div>
        <pre class="code-block"><code>{activeArtifact.content}</code></pre>
      </div>
    {:else if activeArtifact?.type === 'diff'}
      <div class="artifact-diff">
        <div class="artifact-toolbar">
          <span class="artifact-language">diff</span>
          <button class="copy-btn" onclick={() => copyToClipboard(activeArtifact.content)}>
            {copyFeedback ?? 'Copy'}
          </button>
        </div>
        <pre class="diff-block">{#each activeArtifact.content.split('\n') as line}{@const d = renderDiffLine(line)}<span class={d.class}>{d.text}</span>
{/each}</pre>
      </div>
    {:else if activeArtifact?.type === 'json'}
      <div class="artifact-code">
        <div class="artifact-toolbar">
          <span class="artifact-language">JSON</span>
          <button class="copy-btn" onclick={() => copyToClipboard(activeArtifact.content)}>
            {copyFeedback ?? 'Copy'}
          </button>
        </div>
        <pre class="code-block"><code>{activeArtifact.content}</code></pre>
      </div>
    {:else if activeArtifact?.type === 'table'}
      <div class="artifact-table">
        <div class="artifact-toolbar">
          <span class="artifact-language">Table</span>
          <button class="copy-btn" onclick={() => copyToClipboard(activeArtifact.content)}>
            {copyFeedback ?? 'Copy'}
          </button>
        </div>
        <pre class="code-block"><code>{activeArtifact.content}</code></pre>
      </div>
    {:else if activeArtifact?.type === 'html'}
      <!-- WP9-C: HTML artifacts rendered in sandboxed iframe with DOMPurify.
           sandbox="allow-scripts" permits JS but blocks top-level navigation,
           form submission, popups, and access to parent page. -->
      <div class="artifact-html">
        <div class="artifact-toolbar">
          <span class="artifact-language">HTML</span>
          <button class="copy-btn" onclick={() => copyToClipboard(activeArtifact.content)}>
            {copyFeedback ?? 'Copy'}
          </button>
        </div>
        <iframe
          sandbox="allow-scripts"
          srcdoc={sanitizeHtmlArtifact(activeArtifact.content)}
          title="HTML artifact preview"
          class="html-iframe"
        ></iframe>
      </div>
    {/if}
  </div>
</div>
{/if}

<style>
  .artifact-panel {
    border-left: 1px solid var(--color-border-default);
    background: var(--color-bg-elevated-1);
    display: flex;
    flex-direction: column;
    min-width: 300px;
    max-width: 50%;
    overflow: hidden;
  }

  .artifact-tabs {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--color-border-subtle);
    overflow-x: auto;
    flex-shrink: 0;
  }

  .artifact-tabs button {
    padding: var(--spacing-2) var(--spacing-3);
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    cursor: pointer;
    white-space: nowrap;
    transition: color 0.1s, border-color 0.1s;
  }

  .artifact-tabs button.active {
    color: var(--color-interactive-primary);
    border-bottom-color: var(--color-interactive-primary);
  }

  .artifact-tabs button:hover {
    color: var(--color-text-primary);
  }

  .artifact-content {
    flex: 1;
    overflow: auto;
  }

  .artifact-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .artifact-language {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-family-mono);
    text-transform: lowercase;
  }

  .copy-btn {
    padding: 2px 8px;
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
  }
  .copy-btn:hover {
    color: var(--color-text-primary);
  }

  .code-block {
    margin: 0;
    padding: var(--spacing-3);
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    overflow-x: auto;
    color: var(--color-text-primary);
  }

  .diff-block {
    margin: 0;
    padding: var(--spacing-3);
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    line-height: 1.5;
    overflow-x: auto;
    white-space: pre;
  }

  .diff-add {
    color: #22c55e;
    background: color-mix(in srgb, #22c55e 10%, transparent);
  }
  .diff-remove {
    color: #ef4444;
    background: color-mix(in srgb, #ef4444 10%, transparent);
  }
  .diff-hunk {
    color: #3b82f6;
  }
  .diff-context {
    color: var(--color-text-muted);
  }

  .artifact-html {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .html-iframe {
    flex: 1;
    width: 100%;
    min-height: 300px;
    border: none;
    background: #fff;
  }
</style>
