<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
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
  let mutatingBookmark = $state(false);
  let branching = $state(false);

  let playing = $state(false);
  let playbackSpeed = $state(1);
  let playInterval: ReturnType<typeof setInterval> | null = null;

  let bookmarks: SessionBookmark[] = $state([]);
  let newBookmarkLabel = $state('');
  let showBookmarkForm = $state(false);

  let currentEvent = $derived(events[currentIndex] ?? null);

  async function loadReplay() {
    loading = true;
    error = '';
    try {
      const client = await getGhostClient();
      const [eventPage, bookmarkPage] = await Promise.all([
        client.runtimeSessions.events(sessionId, { limit: 500 }),
        client.runtimeSessions.listBookmarks(sessionId),
      ]);
      events = eventPage?.events ?? [];
      total = eventPage?.total ?? 0;
      bookmarks = bookmarkPage?.bookmarks ?? [];
      currentIndex = 0;
    } catch (errorValue: unknown) {
      error = errorValue instanceof Error ? errorValue.message : 'Failed to load session replay';
      events = [];
      bookmarks = [];
      total = 0;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadReplay();
  });

  onDestroy(() => {
    stopPlayback();
  });

  function startPlayback() {
    if (playing || events.length === 0) {
      return;
    }
    playing = true;
    playInterval = setInterval(() => {
      if (currentIndex < events.length - 1) {
        currentIndex += 1;
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
    if (playing) {
      stopPlayback();
    } else {
      startPlayback();
    }
  }

  function setSpeed(speed: number) {
    playbackSpeed = speed;
    if (playing) {
      stopPlayback();
      startPlayback();
    }
  }

  function stepForward() {
    if (currentIndex < events.length - 1) {
      currentIndex += 1;
    }
  }

  function stepBackward() {
    if (currentIndex > 0) {
      currentIndex -= 1;
    }
  }

  function indexForSequence(sequenceNumber: number) {
    return events.findIndex((event) => event.sequence_number === sequenceNumber);
  }

  function jumpToBookmark(bookmark: SessionBookmark) {
    const index = indexForSequence(bookmark.sequence_number);
    if (index >= 0) {
      currentIndex = index;
    }
  }

  async function addBookmark() {
    if (!currentEvent || mutatingBookmark) {
      return;
    }

    mutatingBookmark = true;
    error = '';
    try {
      const client = await getGhostClient();
      const response = await client.runtimeSessions.createBookmark(sessionId, {
        sequence_number: currentEvent.sequence_number,
        label:
          newBookmarkLabel.trim() || `Bookmark at event #${currentEvent.sequence_number}`,
      });
      bookmarks = [...bookmarks, response.bookmark].sort(
        (left, right) => left.sequence_number - right.sequence_number,
      );
      newBookmarkLabel = '';
      showBookmarkForm = false;
    } catch (errorValue: unknown) {
      error = errorValue instanceof Error ? errorValue.message : 'Failed to create bookmark';
    } finally {
      mutatingBookmark = false;
    }
  }

  async function removeBookmark(bookmarkId: string) {
    if (mutatingBookmark) {
      return;
    }

    mutatingBookmark = true;
    error = '';
    try {
      const client = await getGhostClient();
      await client.runtimeSessions.deleteBookmark(sessionId, bookmarkId);
      bookmarks = bookmarks.filter((bookmark) => bookmark.id !== bookmarkId);
    } catch (errorValue: unknown) {
      error = errorValue instanceof Error ? errorValue.message : 'Failed to delete bookmark';
    } finally {
      mutatingBookmark = false;
    }
  }

  async function branchFromHere() {
    if (!currentEvent || branching) {
      return;
    }
    if (
      !confirm(
        `Branch a new session from event #${currentEvent.sequence_number}? Events up to this point will be copied.`,
      )
    ) {
      return;
    }

    branching = true;
    error = '';
    try {
      const client = await getGhostClient();
      const result = await client.runtimeSessions.branch(sessionId, {
        from_sequence_number: currentEvent.sequence_number,
      });
      if (result?.session?.session_id) {
        await goto(`/sessions/${result.session.session_id}`);
      }
    } catch (errorValue: unknown) {
      error = errorValue instanceof Error ? errorValue.message : 'Failed to branch session';
    } finally {
      branching = false;
    }
  }

  let gates = $derived(
    (() => {
      if (!currentEvent) {
        return undefined;
      }
      const attributes = currentEvent.attributes ?? {};
      return [
        { name: 'CB', status: attributes.gate_cb ?? 'unknown', detail: 'Circuit Breaker' },
        { name: 'Depth', status: attributes.gate_depth ?? 'unknown', detail: 'Recursion Depth' },
        { name: 'Damage', status: attributes.gate_damage ?? 'unknown', detail: 'Damage Assessment' },
        { name: 'Cap', status: attributes.gate_cap ?? 'unknown', detail: 'Spending Cap' },
        { name: 'Conv', status: attributes.gate_conv ?? 'unknown', detail: 'Convergence' },
        { name: 'Hash', status: attributes.gate_hash ?? 'unknown', detail: 'Hash Chain' },
      ] as Array<{ name: string; status: 'pass' | 'fail' | 'warning' | 'unknown'; detail: string }>;
    })(),
  );
</script>

{#if loading}
  <div class="loading">Loading replay...</div>
{:else if error && events.length === 0}
  <div class="error-state">
    <p>{error}</p>
    <a href={`/sessions/${sessionId}`}>Back to Session</a>
  </div>
{:else}
  <div class="replay-header">
    <a href={`/sessions/${sessionId}`} class="back-link">Session {sessionId.slice(0, 8)}...</a>
    <h1>Replay</h1>
    <div class="header-actions">
      <button type="button" class="btn-secondary" onclick={() => (showBookmarkForm = !showBookmarkForm)}>
        Bookmark
      </button>
      <button type="button" class="btn-secondary" onclick={branchFromHere} disabled={branching || !currentEvent}>
        {branching ? 'Branching…' : 'Branch'}
      </button>
    </div>
  </div>

  {#if error}
    <p class="inline-error">{error}</p>
  {/if}

  <div class="playback-controls">
    <button type="button" class="control-btn" onclick={stepBackward} disabled={currentIndex <= 0} aria-label="Step back">
      &lt;
    </button>
    <button type="button" class="control-btn play-btn" onclick={togglePlayback} aria-label={playing ? 'Pause' : 'Play'}>
      {playing ? '||' : '>'}
    </button>
    <button type="button" class="control-btn" onclick={stepForward} disabled={currentIndex >= events.length - 1} aria-label="Step forward">
      &gt;
    </button>
    <div class="speed-selector">
      {#each [1, 2, 4] as speed}
        <button type="button" class="speed-btn" class:active={playbackSpeed === speed} onclick={() => setSpeed(speed)}>
          {speed}x
        </button>
      {/each}
    </div>
    <span class="event-counter">{currentIndex + 1} / {events.length} loaded ({total} total)</span>
  </div>

  <div class="timeline-section">
    <TimelineSlider
      min={0}
      max={Math.max(0, events.length - 1)}
      bind:value={currentIndex}
      label="Event"
      onchange={(value) => (currentIndex = value)}
    />
    {#if bookmarks.length > 0 && events.length > 1}
      <div class="bookmark-markers">
        {#each bookmarks as bookmark (bookmark.id)}
          {@const bookmarkIndex = indexForSequence(bookmark.sequence_number)}
          {#if bookmarkIndex >= 0}
            <button
              type="button"
              class="bookmark-marker"
              style={`left: ${(bookmarkIndex / (events.length - 1)) * 100}%`}
              title={bookmark.label}
              onclick={() => jumpToBookmark(bookmark)}
            ></button>
          {/if}
        {/each}
      </div>
    {/if}
  </div>

  {#if showBookmarkForm}
    <div class="bookmark-form">
      <input
        type="text"
        bind:value={newBookmarkLabel}
        placeholder="Bookmark label (optional)"
        onkeydown={(event) => event.key === 'Enter' && addBookmark()}
      />
      <button type="button" class="btn-primary" onclick={addBookmark} disabled={mutatingBookmark || !currentEvent}>
        {mutatingBookmark ? 'Adding…' : 'Add'}
      </button>
      <button type="button" class="btn-text" onclick={() => (showBookmarkForm = false)}>Cancel</button>
    </div>
  {/if}

  {#if bookmarks.length > 0}
    <div class="bookmarks-bar">
      {#each bookmarks as bookmark (bookmark.id)}
        <button
          type="button"
          class="bookmark-chip"
          class:active={currentEvent?.sequence_number === bookmark.sequence_number}
          onclick={() => jumpToBookmark(bookmark)}
        >
          <span class="bm-icon">*</span>
          <span class="bm-label">{bookmark.label}</span>
          <span class="bm-index">#{bookmark.sequence_number}</span>
          <span
            role="button"
            tabindex="0"
            class="bm-remove"
            onclick={(event: MouseEvent) => {
              event.stopPropagation();
              void removeBookmark(bookmark.id);
            }}
            onkeydown={(event: KeyboardEvent) => {
              if (event.key === 'Enter') {
                event.stopPropagation();
                void removeBookmark(bookmark.id);
              }
            }}
          >
            x
          </span>
        </button>
      {/each}
    </div>
  {/if}

  {#if currentEvent}
    <div class="event-detail-grid">
      <section class="card">
        <h2>Event #{currentEvent.sequence_number}</h2>
        <dl class="detail-list">
          <dt>Type</dt><dd>{currentEvent.event_type}</dd>
          <dt>Sender</dt><dd>{currentEvent.sender ?? '—'}</dd>
          <dt>Timestamp</dt><dd>{new Date(currentEvent.timestamp).toLocaleString()}</dd>
          <dt>Tokens</dt><dd>{currentEvent.token_count ?? '—'}</dd>
          <dt>Latency</dt><dd>{currentEvent.latency_ms != null ? `${currentEvent.latency_ms}ms` : '—'}</dd>
          <dt>Privacy</dt><dd>{currentEvent.privacy_level}</dd>
        </dl>
      </section>

      <section class="card">
        <h2>Attributes</h2>
        <pre class="attrs-json">{JSON.stringify(currentEvent.attributes, null, 2)}</pre>
      </section>
    </div>

    <div class="gates-section">
      <GateCheckBar {gates} />
    </div>
  {:else}
    <p class="no-events">No events to display.</p>
  {/if}
{/if}

<style>
  .loading,
  .error-state {
    text-align: center;
    padding: var(--spacing-12);
    color: var(--color-text-muted);
  }

  .inline-error {
    margin-bottom: var(--spacing-3);
    color: var(--color-severity-hard);
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

  .header-actions,
  .speed-selector {
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
    flex-wrap: wrap;
  }

  .control-btn,
  .speed-btn,
  .btn-secondary,
  .btn-primary {
    padding: var(--spacing-2) var(--spacing-3);
    background: var(--color-bg-elevated-2);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-sm);
    color: var(--color-text-primary);
  }

  .speed-btn.active {
    background: var(--color-interactive-primary);
    color: var(--color-interactive-primary-text);
  }

  .event-counter {
    margin-left: auto;
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .timeline-section,
  .bookmark-form,
  .bookmarks-bar,
  .card {
    margin-bottom: var(--spacing-4);
  }

  .bookmark-form {
    display: flex;
    gap: var(--spacing-2);
  }

  .bookmark-form input {
    flex: 1;
    padding: var(--spacing-2);
  }

  .bookmarks-bar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-2);
  }

  .bookmark-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-1);
    padding: var(--spacing-2) var(--spacing-3);
    border: 1px solid var(--color-border-default);
    border-radius: 999px;
    background: var(--color-bg-elevated-1);
  }

  .bookmark-chip.active {
    border-color: var(--color-interactive-primary);
  }

  .bm-remove {
    margin-left: var(--spacing-1);
  }

  .bookmark-markers {
    position: relative;
    height: 16px;
    margin-top: var(--spacing-2);
  }

  .bookmark-marker {
    position: absolute;
    top: 0;
    width: 10px;
    height: 10px;
    border-radius: 999px;
    border: none;
    background: var(--color-interactive-primary);
    transform: translateX(-50%);
  }

  .event-detail-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: var(--spacing-4);
  }

  .card {
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-md);
    padding: var(--spacing-4);
  }

  .detail-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--spacing-2);
  }

  .attrs-json {
    overflow: auto;
    margin: 0;
  }

  .no-events {
    color: var(--color-text-muted);
  }
</style>
