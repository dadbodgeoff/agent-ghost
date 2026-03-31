/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_NAMES = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
];

const LEVEL_LABELS = ['Level 0', 'Level 1', 'Level 2', 'Level 3', 'Level 4'];

let sessionStartedAt = Date.now();

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
      container.innerHTML = '<span class="agent-list-empty">No agents found</span>';
      return;
    }
    container.innerHTML = agents
      .map(
        (a) =>
          `<div class="agent-list-item">` +
          `<span class="agent-name">${a.name || a.id}</span>` +
          `<span class="agent-state">${a.state}</span>` +
          `</div>`
      )
      .join('');
  } catch {
    container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
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
  const platformEl = document.getElementById('platform');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] || `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }
  if (platformEl) platformEl.textContent = 'Gateway';

  renderSignalList(data.signals);

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = 'alert-banner active alert-danger';
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else if (data.level >= 2) {
      alertEl.className = 'alert-banner active alert-warning';
      alertEl.textContent = 'Convergence is elevated. Acknowledge before continuing.';
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function renderSignalList(values: number[]): void {
  const list = document.getElementById('signalList');
  if (!list) return;

  list.innerHTML = SIGNAL_NAMES.map((name, index) => {
    const value = values[index] ?? 0;
    const width = Math.max(0, Math.min(100, value * 100));
    return `<div class="signal-row">
      <span class="signal-name">${name}</span>
      <span class="signal-value">${value.toFixed(2)}</span>
      <div class="signal-bar">
        <div class="signal-bar-fill" style="width:${width.toFixed(0)}%;background:${scoreColor(value)}"></div>
      </div>
    </div>`;
  }).join('');
}

function updateSessionDuration(): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const elapsedSeconds = Math.floor((Date.now() - sessionStartedAt) / 1000);
  const hours = Math.floor(elapsedSeconds / 3600);
  const minutes = Math.floor((elapsedSeconds % 3600) / 60);
  const seconds = elapsedSeconds % 60;
  timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
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

renderSignalList([0, 0, 0, 0, 0, 0, 0]);
updateSessionDuration();
setInterval(updateSessionDuration, 1000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
  }

  await loadSyncStatus();
})();
