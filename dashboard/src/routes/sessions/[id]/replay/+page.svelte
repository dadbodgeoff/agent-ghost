<script lang="ts">
  /**
   * Session replay page (T-2.5.2).
   * TimelineSlider + event detail + GateCheckBar.
   * Lazy-loads events using paginated API.
   */
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api';
  import TimelineSlider from '../../../../components/TimelineSlider.svelte';
  import GateCheckBar from '../../../../components/GateCheckBar.svelte';

  let sessionId = $derived($page.params.id ?? '');
  let events: any[] = $state([]);
  let loading = $state(true);
  let error = $state('');
  let total = $state(0);
  let currentIndex = $state(0);

  let currentEvent = $derived(events[currentIndex] ?? null);

  onMount(async () => {
    try {
      const data = await api.get(`/api/sessions/${sessionId}/events?limit=500`);
      events = data?.events ?? [];
      total = data?.total ?? 0;
    } catch (e: any) {
      error = e.message || 'Failed to load session events';
    }
    loading = false;
  });

  function handleSliderChange(value: number) {
    currentIndex = value;
  }

  // Build gate states from current event attributes
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
  <div class="loading">Loading replay…</div>
{:else if error}
  <div class="error-state">
    <p>{error}</p>
    <a href="/sessions/{sessionId}">← Back to Session</a>
  </div>
{:else}
  <div class="replay-header">
    <a href="/sessions/{sessionId}" class="back-link">← Session {sessionId.slice(0, 8)}…</a>
    <h1>Replay</h1>
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
  </div>

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

  .replay-header { margin-bottom: var(--spacing-4); }

  .replay-header h1 {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin: var(--spacing-1) 0;
  }

  .timeline-section {
    margin-bottom: var(--spacing-6);
    padding: var(--spacing-4);
    background: var(--color-bg-elevated-1);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-default);
  }

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

  .gates-section {
    margin-bottom: var(--spacing-6);
  }

  .no-events {
    text-align: center;
    color: var(--color-text-muted);
    padding: var(--spacing-8);
  }

  @media (max-width: 640px) {
    .event-detail-grid { grid-template-columns: 1fr; }
  }
</style>
