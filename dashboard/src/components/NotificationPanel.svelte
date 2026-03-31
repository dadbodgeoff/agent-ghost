<script lang="ts">
  /**
   * NotificationPanel — Real-time notification bell + dropdown (Phase 2, Task 3.10).
   *
   * Maps WS events to notifications with severity coloring, click navigation,
   * mark-all-read, and native OS notifications for critical events (Tauri only).
   */
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { wsStore, type WsMessage } from '$lib/stores/websocket.svelte';
  import { getRuntime } from '$lib/platform/runtime';

  interface AppNotification {
    id: string;
    type: 'agent_state' | 'safety_alert' | 'approval_request' | 'cost_warning' | 'system';
    severity: 'info' | 'warning' | 'critical';
    title: string;
    message: string;
    timestamp: string;
    read: boolean;
    actionHref?: string;
    agentId?: string;
  }

  let notifications = $state<AppNotification[]>([]);
  let panelOpen = $state(false);
  let unsubs: Array<() => void> = [];

  let unreadCount = $derived(notifications.filter(n => !n.read).length);

  const STORAGE_KEY = 'ghost-notifications';
  const MAX_NOTIFICATIONS = 100;

  onMount(() => {
    loadFromStorage();

    unsubs.push(
      wsStore.on('AgentStateChange', (msg: WsMessage) => {
        addNotification({
          type: 'agent_state',
          severity: 'info',
          title: `Agent ${(msg as any).agent_id ?? 'unknown'} state changed`,
          message: `New state: ${(msg as any).status ?? (msg as any).new_state ?? 'unknown'}`,
          actionHref: `/agents/${(msg as any).agent_id}`,
          agentId: (msg as any).agent_id as string,
        });
      }),
      wsStore.on('KillSwitchActivation', (msg: WsMessage) => {
        addNotification({
          type: 'safety_alert',
          severity: 'critical',
          title: 'Kill Switch Activated',
          message: (msg as any).reason ?? 'No reason provided',
          actionHref: '/security',
        });
      }),
      wsStore.on('InterventionChange', (msg: WsMessage) => {
        addNotification({
          type: 'safety_alert',
          severity: 'warning',
          title: 'Intervention Level Changed',
          message: `Agent ${(msg as any).agent_id}: level → ${(msg as any).new_level ?? 'unknown'}`,
          actionHref: '/convergence',
          agentId: (msg as any).agent_id as string,
        });
      }),
      wsStore.on('ProposalUpdated', (msg: WsMessage) => {
        const proposalId = (msg as any).proposal_id ?? '';
        const change = (msg as any).change ?? 'updated';
        const status = (msg as any).status ?? 'updated';
        addNotification({
          type: 'approval_request',
          severity: 'info',
          title: change === 'created' ? 'Proposal Created' : 'Proposal Updated',
          message:
            change === 'created'
              ? `Proposal ${proposalId} entered review as ${status}.`
              : `Proposal ${proposalId} moved to ${status}.`,
          actionHref: proposalId ? `/goals/${proposalId}` : '/goals',
        });
      }),
    );
  });

  onDestroy(() => {
    for (const unsub of unsubs) unsub();
    unsubs = [];
  });

  function addNotification(partial: Omit<AppNotification, 'id' | 'timestamp' | 'read'>) {
    const notification: AppNotification = {
      ...partial,
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
      read: false,
    };

    notifications = [notification, ...notifications].slice(0, MAX_NOTIFICATIONS);
    persistToStorage();
    pushNativeNotification(notification);
  }

  async function pushNativeNotification(n: AppNotification) {
    if (n.severity !== 'critical') return;
    try {
      const runtime = await getRuntime();
      if (!runtime.isDesktop()) return;
      await runtime.sendNotification({
        title: n.title,
        body: n.message,
      });
    } catch {
      // Non-fatal
    }
  }

  function markAllRead() {
    notifications = notifications.map(n => ({ ...n, read: true }));
    persistToStorage();
  }

  function markRead(id: string) {
    notifications = notifications.map(n =>
      n.id === id ? { ...n, read: true } : n
    );
    persistToStorage();
  }

  function handleNotificationClick(n: AppNotification) {
    markRead(n.id);
    panelOpen = false;
    if (n.actionHref) {
      goto(n.actionHref);
    }
  }

  function relativeTime(iso: string): string {
    try {
      const diff = Date.now() - new Date(iso).getTime();
      const mins = Math.floor(diff / 60000);
      if (mins < 1) return 'now';
      if (mins < 60) return `${mins}m ago`;
      const hrs = Math.floor(mins / 60);
      if (hrs < 24) return `${hrs}h ago`;
      return `${Math.floor(hrs / 24)}d ago`;
    } catch { return ''; }
  }

  function severityColor(severity: string): string {
    switch (severity) {
      case 'critical': return 'var(--color-severity-hard)';
      case 'warning': return 'var(--color-severity-active)';
      default: return 'var(--color-interactive-primary)';
    }
  }

  function loadFromStorage() {
    if (typeof localStorage === 'undefined') return;
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        notifications = JSON.parse(stored);
      }
    } catch { /* start fresh */ }
  }

  function persistToStorage() {
    if (typeof localStorage === 'undefined') return;
    localStorage.setItem(STORAGE_KEY, JSON.stringify(notifications));
  }
</script>

<div class="notification-wrapper">
  <button
    class="bell-button"
    onclick={() => panelOpen = !panelOpen}
    aria-label={`Notifications (${unreadCount} unread)`}
  >
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"></path>
      <path d="M13.73 21a2 2 0 0 1-3.46 0"></path>
    </svg>
    {#if unreadCount > 0}
      <span class="badge">{unreadCount > 99 ? '99+' : unreadCount}</span>
    {/if}
  </button>

  {#if panelOpen}
    <div class="panel-overlay" onclick={() => panelOpen = false} role="presentation"></div>
    <div class="notification-panel" role="dialog" aria-label="Notifications">
      <div class="panel-header">
        <h3>Notifications</h3>
        {#if unreadCount > 0}
          <button class="mark-read-btn" onclick={markAllRead}>Mark all read</button>
        {/if}
      </div>

      {#if notifications.length === 0}
        <div class="panel-empty">No notifications yet</div>
      {:else}
        <ul class="notification-list">
          {#each notifications as n}
            <li>
              <button
                type="button"
                class="notification-item"
                class:unread={!n.read}
                onclick={() => handleNotificationClick(n)}
              >
                <div class="notification-indicator" style="background: {severityColor(n.severity)}"></div>
                <div class="notification-body">
                  <div class="notification-title">{n.title}</div>
                  <div class="notification-message">{n.message}</div>
                  <div class="notification-time">{relativeTime(n.timestamp)}</div>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>

<style>
  .notification-wrapper {
    position: relative;
  }

  .bell-button {
    position: relative;
    background: none;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: var(--spacing-1);
    border-radius: var(--radius-sm);
    transition: color 0.1s;
  }
  .bell-button:hover {
    color: var(--color-text-primary);
  }

  .badge {
    position: absolute;
    top: -4px;
    right: -6px;
    background: var(--color-severity-hard);
    color: white;
    font-size: 10px;
    font-weight: var(--font-weight-bold);
    min-width: 16px;
    height: 16px;
    line-height: 16px;
    text-align: center;
    border-radius: 8px;
    padding: 0 3px;
  }

  .panel-overlay {
    position: fixed;
    inset: 0;
    z-index: 900;
  }

  .notification-panel {
    position: absolute;
    top: 100%;
    right: 0;
    width: 360px;
    max-height: 480px;
    background: var(--color-bg-elevated-1);
    border: 1px solid var(--color-border-default);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-elevated-3);
    z-index: 950;
    overflow: hidden;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--spacing-3);
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .panel-header h3 {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-bold);
  }

  .mark-read-btn {
    font-size: var(--font-size-xs);
    color: var(--color-interactive-primary);
    background: none;
    border: none;
    cursor: pointer;
  }
  .mark-read-btn:hover { text-decoration: underline; }

  .panel-empty {
    padding: var(--spacing-8);
    text-align: center;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }

  .notification-list {
    list-style: none;
    padding: 0;
    margin: 0;
    max-height: 420px;
    overflow-y: auto;
  }

  .notification-item {
    display: flex;
    gap: var(--spacing-2);
    width: 100%;
    padding: var(--spacing-2) var(--spacing-3);
    background: transparent;
    border: none;
    text-align: left;
    cursor: pointer;
    border-bottom: 1px solid var(--color-border-subtle);
    transition: background 0.1s;
  }
  .notification-item:hover {
    background: var(--color-bg-elevated-2);
  }
  .notification-item.unread {
    background: color-mix(in srgb, var(--color-interactive-primary) 5%, transparent);
  }

  .notification-indicator {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
    margin-top: 6px;
  }

  .notification-body {
    flex: 1;
    min-width: 0;
  }

  .notification-title {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .notification-message {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    margin-top: 2px;
  }

  .notification-time {
    font-size: 10px;
    color: var(--color-text-muted);
    opacity: 0.7;
    margin-top: 2px;
  }
</style>
