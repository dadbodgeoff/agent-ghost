/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

interface PopupScorePayload {
  score: number;
  level: number;
  signals: number[];
}

const SIGNAL_LABELS = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
  'Behavioral Anomaly',
];

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

function renderEmptyAgentList(message: string): void {
  const container = document.getElementById('agentList');
  if (!container) return;

  container.replaceChildren();
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = message;
  container.append(empty);
}

function renderAgentList(agents: Array<{ id: string; name: string; state: string }>): void {
  const container = document.getElementById('agentList');
  if (!container) return;

  container.replaceChildren();
  if (agents.length === 0) {
    renderEmptyAgentList('No agents found');
    return;
  }

  for (const agent of agents) {
    const row = document.createElement('div');
    row.className = 'agent-list-item';

    const name = document.createElement('span');
    name.className = 'agent-name';
    name.textContent = agent.name || agent.id;

    const state = document.createElement('span');
    state.className = 'agent-state';
    state.textContent = agent.state;

    row.append(name, state);
    container.append(row);
  }
}

function renderSignals(signals: number[]): void {
  const list = document.getElementById('signalList');
  if (!list) return;

  list.replaceChildren();
  SIGNAL_LABELS.forEach((label, index) => {
    const value = signals[index] ?? 0;

    const row = document.createElement('div');
    row.className = 'signal-row';

    const name = document.createElement('span');
    name.className = 'signal-name';
    name.textContent = label;

    const valueWrap = document.createElement('span');
    valueWrap.style.display = 'flex';
    valueWrap.style.alignItems = 'center';

    const valueEl = document.createElement('span');
    valueEl.className = 'signal-value';
    valueEl.textContent = value.toFixed(2);

    const bar = document.createElement('span');
    bar.className = 'signal-bar';

    const fill = document.createElement('span');
    fill.className = 'signal-bar-fill';
    fill.style.width = `${Math.max(0, Math.min(1, value)) * 100}%`;
    fill.style.background = value >= 0.7 ? '#ef4444' : value >= 0.4 ? '#f59e0b' : '#22c55e';
    bar.append(fill);

    valueWrap.append(valueEl, bar);
    row.append(name, valueWrap);
    list.append(row);
  });
}

function updateSessionDuration(sessionStart: number): void {
  const sessionDurationEl = document.getElementById('sessionDuration');
  if (!sessionDurationEl) return;

  const elapsedMinutes = Math.floor((Date.now() - sessionStart) / 60000);
  sessionDurationEl.textContent = `${elapsedMinutes}m`;
}

/**
 * Fetch and render the agent list from the gateway.
 */
async function loadAgentList(): Promise<void> {
  try {
    renderAgentList(await getAgents());
  } catch {
    renderEmptyAgentList('Unable to load agents');
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

function updateUI(data: PopupScorePayload): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignals(data.signals);

  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (data.level >= 3) {
    alertEl.className = `alert-banner active ${data.level >= 4 ? 'alert-danger' : 'alert-warning'}`;
    alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    return;
  }

  alertEl.className = 'alert-banner';
  alertEl.textContent = '';
}

chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (chrome.runtime.lastError) {
    return;
  }

  if (response && response.score !== undefined) {
    const level = response.score > 0.85 ? 4 :
      response.score > 0.7 ? 3 :
      response.score > 0.5 ? 2 :
      response.score > 0.3 ? 1 : 0;

    updateUI({
      score: response.score,
      level,
      signals: Array.from({ length: SIGNAL_LABELS.length }, () => 0),
    });
  }
});

const sessionStart = Date.now();
updateSessionDuration(sessionStart);
setInterval(() => {
  updateSessionDuration(sessionStart);
}, 60_000);

(async () => {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = auth.authenticated ? 'Gateway' : 'Offline';
  }

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    renderEmptyAgentList('Not connected to gateway');
  }

  await loadSyncStatus();
})();
