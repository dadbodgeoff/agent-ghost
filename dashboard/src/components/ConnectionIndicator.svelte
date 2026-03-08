<script lang="ts">
  /**
   * ConnectionIndicator — shows WebSocket connection state.
   * Green dot = connected, red = disconnected, yellow = reconnecting.
   * Ref: T-1.14.1, T-X.15, §9.3
   */

  let {
    state = 'disconnected' as
      | 'connected'
      | 'connecting'
      | 'reconnecting'
      | 'disconnected'
      | 'follower',
  }: {
    state?: 'connected' | 'connecting' | 'reconnecting' | 'disconnected' | 'follower';
  } = $props();

  const STATE_MAP: Record<string, { color: string; label: string }> = {
    connected:    { color: 'var(--color-severity-normal)', label: 'Connected' },
    connecting:   { color: 'var(--color-severity-soft)',   label: 'Connecting' },
    reconnecting: { color: 'var(--color-severity-soft)',   label: 'Reconnecting' },
    disconnected: { color: 'var(--color-severity-hard)',   label: 'Disconnected' },
    follower:     { color: 'var(--color-text-muted)',      label: 'Follower' },
  };

  let info = $derived(STATE_MAP[state] || STATE_MAP.disconnected);
</script>

<span
  class="indicator"
  title={info.label}
  role="status"
  aria-label="WebSocket {info.label}"
>
  <span
    class="dot"
    class:pulse={state === 'disconnected' || state === 'reconnecting'}
    style="background: {info.color}"
    aria-hidden="true"
  ></span>
  <span class="label">{info.label}</span>
</span>

<style>
  .indicator {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-1);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-full);
    flex-shrink: 0;
  }

  .dot.pulse {
    animation: pulse-dot 2s ease-in-out infinite;
  }

  .label {
    font-weight: var(--font-weight-medium);
  }

  @keyframes pulse-dot {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.3; }
  }
</style>
