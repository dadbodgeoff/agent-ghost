<script lang="ts">
  /**
   * Session replay with bookmarks and branching (Phase 3, Task 3.9).
   * TimelineSlider + event detail + GateCheckBar + bookmarks + play/pause + branch.
   */
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { getGhostClient } from '$lib/ghost-client';
  import TimelineSlider from '../../../../components/TimelineSlider.svelte';
  import GateCheckBar from '../../../../components/GateCheckBar.svelte';
  import type { SessionBookmark, SessionEvent } from '@ghost/sdk';

  let sessionId = $derived($page.params.id ?? '');
  let events: SessionEvent[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let total = $state(0);
  let currentIndex = $state(0);

  // Playback
  let playing = $state(false);
  let playbackSpeed = $state(1);
  let playInterval: ReturnType<typeof setInterval> | null = null;

  // Bookmarks
  let bookmarks: SessionBookmark[] = $state([]);
  let newBookmarkLabel = $state('');
  let showBookmarkForm = $state(false);

  let currentEvent = $derived(events[currentIndex] ?? null);

  onMount(async () => {
    try {
      const client = await getGhostClient();
      const data = await client.runtimeSessions.events(sessionId, { limit: 500 });
      events = data?.events ?? [];
      total = data?.total ?? 0;
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load session events';
    }
    loading = false;

    // Load bookmarks
    try {
      const client = await getGhostClient();
      const bData = await client.runtimeSessions.listBookmarks(sessionId);
      bookmarks = bData?.bookmarks ?? [];
    } catch { /* bookmarks not supported yet — non-fatal */ }
  });

  onDestroy(() => {
    stopPlayback();
  });

  function handleSliderChange(value: number) {
    currentIndex = value;
  }

  function startPlayback() {
    if (playing) return;
    playing = true;
    playInterval = setInterval(() => {
      if (currentIndex < events.length - 1) {
        currentIndex++;
      } else {
        stopPlayback();
      }
    }, 1000 / playbackSpeed);
  }

  function stopPlayback() {
    playing = false;
    if (playInterval) {
      clearInterval(playInterval);
      playInterval = null;
    }
  }

  function togglePlayback() {
    if (playing) stopPlayback();
    else startPlayback();
  }

  function setSpeed(speed: number) {
    playbackSpeed = speed;
    if (playing) {
      stopPlayback();
      startPlayback();
    }
  }

  function stepForward() {
    if (currentIndex < events.length - 1) currentIndex++;
  }

  function stepBackward() {
    if (currentIndex > 0) currentIndex--;
  }

  async function addBookmark() {
    const label = newBookmarkLabel.trim() || `Bookmark at event #${currentIndex}`;
    const bookmark: SessionBookmark = {
      id: crypto.randomUUID(),
      eventIndex: currentIndex,
      label,
      createdAt: new Date().toISOString(),
    };

    try {
      const client = await getGhostClient();
      await client.runtimeSessions.createBookmark(sessionId, bookmark);
    } catch { /* persist failed — keep locally */ }

    bookmarks = [...bookmarks, bookmark].sort((a, b) => a.eventIndex - b.eventIndex);
    newBookmarkLabel = '';
    showBookmarkForm = false;
  }

  function jumpToBookmark(bm: SessionBookmark) {
    currentIndex = bm.eventIndex;
  }

  function removeBookmark(id: string) {
    bookmarks = bookmarks.filter(b => b.id !== id);
    void getGhostClient()
      .then((client) => client.runtimeSessions.deleteBookmark(sessionId, id))
      .catch(() => {});
  }

  async function branchFromHere() {
    if (!confirm(`Branch a new session from event #${currentIndex}? Events up to this point will be copied.`)) return;

    try {
      const client = await getGhostClient();
      const result = await client.runtimeSessions.branch(sessionId, {
        from_event_index: currentIndex,
      });
      if (result?.session_id) {
        goto(`/sessions/${result.session_id}`);
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to branch session';
    }
  }

  // Build gate states from current event attributes.
  let gates = $derived((() => {
    if (!currentEvent) return undefined;
    const attrs = currentEvent.attributes ?? {};
    return [
      { name: 'CB', status: attrs.gate_cb ?? 'unknown', detail: 'Circuit Breaker' },
      { name: 'Depth', status: attrs.gate_depth ?? 'unknown', detail: 'Recursion Depth' },
      { name: 'Damage', status: attrs.gate_damage ?? 'unknown', detail: 'Damage Assessment' },
      { name: 'Cap', status: attrs.gate_cap ?? 'unknown', detail: 'Spending Cap' },
      { name: 'Conv', status: attrs.gate_conv ?? 'unknown', detail: 'Convergence' },
      { name: 'Hash', status: attrs.gate_hash ?? 'unknown', detail: 'Hash Chain' },
    ] as Array<{ name: string; status: 'pass' | 'fail' | 'warning' | 'unknown'; detail: string }>;
  })());
</script>

{#if loading}
  <div class="loading">Loading replay...</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <a href={`/sessions/${sessionId}`}>Back to Session</a>
  </div>
{:else}
  <div class="replay-header">
    <a href={`/sessions/${sessionId}`} class="back-link">Session {sessionId.slice(0, 8)}...</a>
    <h1>Replay</h1>
    <div class="header-actions">
      <button class="btn-secondary" onclick={() => (showBookmarkForm = !showBookmarkForm)}>
        Bookmark
      </button>
      <button class="btn-secondary" onclick={branchFromHere}>
        Branch
      </button>
    </div>
  </div>

  <!-- Playback Controls -->
  <div class="playback-controls">
    <button class="control-btn" onclick={stepBackward} disabled={currentIndex <= 0} aria-label="Step back">
      &lt;
    </button>
    <button class="control-btn play-btn" onclick={togglePlayback} aria-label={playing ? 'Pause' : 'Play'}>
      {playing ? '||' : '>'}
    </button>
    <button class="control-btn" onclick={stepForward} disabled={currentIndex >= events.length - 1} aria-label="Step forward">
      &gt;
    </button>
    <div class="speed-selector">
      {#each [1, 2, 4] as speed}
        <button
          class="speed-btn"
          class:active={playbackSpeed === speed}
          onclick={() => setSpeed(speed)}
        >{speed}x</button>
      {/each}
    </div>
    <span class="event-counter">{currentIndex + 1} / {events.length}</span>
  </div>

  <!-- Timeline Slider -->
  <div class="timeline-section">
    <TimelineSlider
      min={0}
      max={Math.max(0, events.length - 1)}
      bind:value={currentIndex}
      label="Event"
      onchange={handleSliderChange}
    />
    <!-- Bookmark markers on timeline -->
    {#if bookmarks.length > 0 && events.length > 1}
      <div class="bookmark-markers">
        {#each bookmarks as bm (bm.id)}
          <button
            class="bookmark-marker"
            style="left: {(bm.eventIndex / (events.length - 1)) * 100}%"
            title={bm.label}
            onclick={() => jumpToBookmark(bm)}
          ></button>
        {/each}
      </div>
    {/if}
  </div>

  <!-- Bookmark Form -->
  {#if showBookmarkForm}
    <div class="bookmark-form">
      <input
        type="text"
        bind:value={newBookmarkLabel}
        placeholder="Bookmark label (optional)"
        onkeydown={(e) => e.key === 'Enter' && addBookmark()}
      />
      <button class="btn-primary" onclick={addBookmark}>Add</button>
      <button class="btn-text" onclick={() => (showBookmarkForm = false)}>Cancel</button>
    </div>
  {/if}

  <!-- Bookmarks List -->
  {#if bookmarks.length > 0}
    <div class="bookmarks-bar">
      {#each bookmarks as bm (bm.id)}
        <button class="bookmark-chip" class:active={currentIndex === bm.eventIndex} onclick={() => jumpToBookmark(bm)}>
          <span class="bm-icon">*</span>
          <span class="bm-label">{bm.label}</span>
          <span class="bm-index">#{bm.eventIndex}</span>
          <span role="button" tabindex="0" class="bm-remove" onclick={(e: MouseEvent) => { e.stopPropagation(); removeBookmark(bm.id); }} onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') { e.stopPropagation(); removeBookmark(bm.id); } }}>x</span>
        </button>
      {/each}
    </div>
  {/if}

  {#if currentEvent}
    <!-- Current Event Detail -->
    <div class="event-detail-grid">
      <section class="card">
        <h2>Event #{currentEvent.sequence_number}</h2>
        <dl class="detail-list">
          <dt>Type</dt><dd>{currentEvent.event_type}</dd>
          <dt>Sender</dt><dd>{currentEvent.sender ?? '—'}</dd>
          <dt>Timestamp</dt><dd>{new Date(currentEvent.timestamp).toLocaleString()}</dd>
          <dt>Tokens</dt><dd>{currentEvent.token_count ?? '—'}</dd>
          <dt>Latency</dt><dd>{currentEvent.latency_ms != null ? currentEvent.latency_ms + 'ms' : '—'}</dd>
          <dt>Privacy</dt><dd>{currentEvent.privacy_level}</dd>
        </dl>
      </section>

      <section class="card">
        <h2>Attributes</h2>
        <pre class="attrs-json">{JSON.stringify(currentEvent.attributes, null, 2)}</pre>
      </section>
    </div>

    <!-- Gate Check Bar -->
    <div class="gates-section">
      <GateCheckBar {gates} />
    </div>
  {:else}
    <p class="no-events">No events to display.</p>
  {/if}
{/if}

<style>
  .loading, .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .back-link {
    font-size: var(--font-size-sm);
    color: var(--color-interactive-primary);
    text-decoration: none;
  }

  .replay-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .replay-header h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin: 0;
    flex: 1;
  }

  .header-actions {
    display: flex;
    gap: var(--spacing-2);
  }

  .playback-controls {
    display: flex;
    align-items: center;
    gap: var(--spacing-2);
    margin-bottom: var(--spacing-4);
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .control-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
    cursor: pointer;
  }

  .control-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .play-btn {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border-color: var(--color-interactive-primary);
  }

  .speed-selector {
    display: flex;
    gap: var(--spacing-1);
    margin-left: var(--spacing-2);
  }

  .speed-btn {
    padding: var(--spacing-1) var(--spacing-2);
    background: none;
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
    cursor: pointer;
  }

  .speed-btn.active {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
    border-color: var(--color-interactive-primary);
  }

  .event-counter {
    margin-left: auto;
    font-size: var(--font-size-xs);
    font-family: var(--font-family-mono);
    color: var(--color-text-muted);
  }

  .timeline-section {
    position: relative;
    margin-bottom: var(--spacing-4);
    padding: var(--spacing-4);
    background: var(--color-bg-elevated-1);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
  }

  .bookmark-markers {
    position: relative;
    height: 8px;
    margin-top: var(--spacing-2);
  }

  .bookmark-marker {
    position: absolute;
    width: 8px;
    height: 8px;
    background: var(--color-severity-soft);
    border: 1px solid var(--color-text-inverse);
    border-radius: 50%;
    transform: translateX(-50%);
    cursor: pointer;
    padding: 0;
  }

  .bookmark-marker:hover {
    background: var(--color-interactive-primary);
    transform: translateX(-50%) scale(1.3);
  }

  .bookmark-form {
    display: flex;
    gap: var(--spacing-2);
    align-items: center;
    margin-bottom: var(--spacing-3);
    padding: var(--spacing-3);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
  }

  .bookmark-form input {
    flex: 1;
    padding: var(--spacing-2);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .bookmarks-bar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-2);
    margin-bottom: var(--spacing-4);
  }

  .bookmark-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-1);
    padding: var(--spacing-1) var(--spacing-2);
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    cursor: pointer;
    color: var(--color-text-primary);
  }

  .bookmark-chip.active {
    border-color: var(--color-interactive-primary);
    background: var(--color-surface-selected);
  }

  .bm-icon { color: var(--color-severity-soft); }
  .bm-index { color: var(--color-text-muted); font-family: var(--font-family-mono); }

  .bm-remove {
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 0 2px;
    font-size: var(--font-size-xs);
  }
  .bm-remove:hover { color: var(--color-severity-hard); }

  .event-detail-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--spacing-4);
    margin-bottom: var(--spacing-4);
  }

  .card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .card h2 {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-semibold);
    text-transform: uppercase;
    letter-spacing: var(--letter-spacing-wide);
    color: var(--color-text-muted);
    margin-bottom: var(--spacing-3);
  }

  .detail-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--spacing-1) var(--spacing-3);
    margin: 0;
  }

  .detail-list dt { font-size: var(--font-size-xs); color: var(--color-text-muted); }
  .detail-list dd { font-size: var(--font-size-sm); margin: 0; }

  .attrs-json {
    font-family: var(--font-family-mono);
    font-size: var(--font-size-xs);
    background: var(--color-bg-elevated-2);
    padding: var(--spacing-3);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    max-height: 200px;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .gates-section { margin-bottom: var(--spacing-6); }

  .no-events {
    text-align: center;
    color: var(--color-text-muted);
    padding: var(--spacing-8);
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
    padding: var(--spacing-1) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .btn-text {
    background: none;
    border: none;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  @media (max-width: 640px) {
    .event-detail-grid { grid-template-columns: 1fr; }
  }
</style>
