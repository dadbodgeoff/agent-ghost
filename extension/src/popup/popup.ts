/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

/**
 * Update the connection indicator (statusDot + statusLabel).
 */
function updateConnectionIndicator(connected: boolean): void {
  const dot = document.getElementById('statusDot');
  const label = document.getElementById('statusLabel');
  if (dot) {
    dot.classList.remove('connected', 'disconnected');
    dot.classList.add(connected ? 'connected' : 'disconnected');
  }
  if (label) {
    label.classList.remove('connected', 'disconnected');
    label.classList.add(connected ? 'connected' : 'disconnected');
    label.textContent = connected ? 'Connected' : 'Disconnected';
  }
}

/**
 * Fetch and render the agent list from the gateway.
 */
async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      renderAgentListMessage(container, 'No agents found');
      return;
    }

    container.replaceChildren(
      ...agents.map((agent) => {
        const row = document.createElement('div');
        row.className = 'agent-list-item';

        const name = document.createElement('span');
        name.className = 'agent-name';
        name.textContent = agent.name || agent.id;

        const state = document.createElement('span');
        state.className = 'agent-state';
        state.textContent = agent.state;

        row.append(name, state);
        return row;
      }),
    );
  } catch {
    renderAgentListMessage(container, 'Unable to load agents');
  }
}

/**
 * Load and display the last sync time from storage.
 */
async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!el) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const ts = stored['ghost-last-sync'];
  if (ts && typeof ts === 'number') {
    el.textContent = new Date(ts).toLocaleTimeString();
  } else {
    el.textContent = 'never';
  }
}

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignals(data.signals);

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = `alert-banner active ${data.level >= 4 ? 'alert-danger' : 'alert-warning'}`;
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function renderSignals(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  const labels = [
    'Novelty drift',
    'Latency spike',
    'Retry pressure',
    'Context churn',
    'Agent divergence',
    'Memory load',
    'Safety risk',
  ];

  const rows = labels.map((label, index) => {
    const value = signals[index] ?? 0;
    const row = document.createElement('div');
    row.className = 'signal-row';

    const name = document.createElement('span');
    name.className = 'signal-name';
    name.textContent = label;

    const valueWrap = document.createElement('span');
    valueWrap.style.display = 'flex';
    valueWrap.style.alignItems = 'center';

    const valueText = document.createElement('span');
    valueText.className = 'signal-value';
    valueText.textContent = value.toFixed(2);

    const bar = document.createElement('span');
    bar.className = 'signal-bar';

    const fill = document.createElement('span');
    fill.className = 'signal-bar-fill';
    fill.style.width = `${Math.max(0, Math.min(1, value)) * 100}%`;
    fill.style.background = value > 0.7 ? '#ef4444' : value > 0.4 ? '#f59e0b' : '#22c55e';

    bar.append(fill);
    valueWrap.append(valueText, bar);
    row.append(name, valueWrap);
    return row;
  });

  container.replaceChildren(...rows);
}

function renderAgentListMessage(container: HTMLElement, message: string): void {
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = message;
  container.replaceChildren(empty);
}

function updateSessionInfo(authenticated: boolean): void {
  const duration = document.getElementById('sessionDuration');
  if (duration) {
    duration.textContent = '0m';
  }

  const platform = document.getElementById('platform');
  if (platform) {
    platform.textContent = authenticated ? 'Gateway connected' : 'Offline / unauthenticated';
  }
}

// Request score from background
chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (response && response.score !== undefined) {
    const level = response.score > 0.85 ? 4 :
                  response.score > 0.7 ? 3 :
                  response.score > 0.5 ? 2 :
                  response.score > 0.3 ? 1 : 0;
    updateUI({
      score: response.score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  }
});

// Session timer
const sessionStart = Date.now();
function refreshSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

refreshSessionTimer();
setInterval(refreshSessionTimer, 60000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);
  updateSessionInfo(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      renderAgentListMessage(container, 'Not connected to gateway');
    }
  }

  await loadSyncStatus();
})();
