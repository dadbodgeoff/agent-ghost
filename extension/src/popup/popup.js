/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync.js';
import { getAgents } from '../background/gateway-client.js';

const LEVEL_LABELS = ['Level 0', 'Level 1', 'Level 2', 'Level 3', 'Level 4'];
const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'];
const SIGNAL_COUNT = 7;
const SIGNAL_NAMES = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
];

function updateConnectionIndicator(connected) {
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

function setAgentListMessage(message) {
  const container = document.getElementById('agentList');
  if (!container) return;
  container.replaceChildren();
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = message;
  container.append(empty);
}

async function loadAgentList() {
  const container = document.getElementById('agentList');
  if (!container) return;

  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      setAgentListMessage('No agents found');
      return;
    }
    container.replaceChildren();
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
  } catch {
    setAgentListMessage('Unable to load agents');
  }
}

async function loadSyncStatus() {
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

function normalizeSignals(signals) {
  return Array.from({ length: SIGNAL_COUNT }, (_, index) => {
    const value = signals[index];
    if (typeof value !== 'number' || Number.isNaN(value)) {
      return 0;
    }
    return Math.min(1, Math.max(0, value));
  });
}

function scoreColor(value) {
  return value < 0.3 ? '#22c55e' : value < 0.5 ? '#eab308' : value < 0.7 ? '#f97316' : '#ef4444';
}

function renderSignalList() {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.replaceChildren();
  SIGNAL_NAMES.forEach((name, index) => {
    const row = document.createElement('div');
    row.className = 'signal-row';

    const label = document.createElement('span');
    label.className = 'signal-name';
    label.textContent = name;

    const value = document.createElement('span');
    value.className = 'signal-value';
    value.id = `signal-value-${index}`;
    value.textContent = '0.000';

    const bar = document.createElement('div');
    bar.className = 'signal-bar';

    const fill = document.createElement('div');
    fill.className = 'signal-bar-fill';
    fill.id = `signal-bar-${index}`;
    fill.style.width = '0%';

    bar.append(fill);
    row.append(label, value, bar);
    container.append(row);
  });
}

function updateAlert(level) {
  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      level >= 4
        ? 'Intervention Level 4 detected. External escalation may be active.'
        : 'Intervention Level 3 detected. Session may require intervention.';
    return;
  }

  if (level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2 detected. Acknowledgment may be required.';
    return;
  }

  alertEl.className = 'alert-banner';
  alertEl.textContent = '';
}

function updateUI(data) {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl instanceof HTMLElement) {
    scoreEl.textContent = data.score.toFixed(2);
    scoreEl.style.color = scoreColor(data.score);
  }
  if (levelEl) {
    const level = Math.min(Math.max(data.level, 0), LEVEL_LABELS.length - 1);
    levelEl.textContent = LEVEL_LABELS[level] ?? `Level ${level}`;
    levelEl.className = `level-badge ${LEVEL_CLASSES[level] ?? 'level-0'}`;
  }

  const signals = normalizeSignals(data.signals);
  signals.forEach((val, i) => {
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`);
    if (valueEl) valueEl.textContent = val.toFixed(3);
    if (barEl instanceof HTMLElement) {
      barEl.style.width = `${(val * 100).toFixed(0)}%`;
      barEl.style.background = scoreColor(val);
    }
  });

  updateAlert(data.level);
}

chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (chrome.runtime.lastError) {
    return;
  }
  if (response && typeof response.score === 'number') {
    const level = response.score > 0.85 ? 4 : response.score > 0.7 ? 3 : response.score > 0.5 ? 2 : response.score > 0.3 ? 1 : 0;
    updateUI({
      score: response.score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  }
});

const sessionStart = Date.now();
function updateSessionTimer() {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}
updateSessionTimer();
setInterval(updateSessionTimer, 60000);

(async () => {
  renderSignalList();
  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    setAgentListMessage('Not connected to gateway');
  }

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = auth.gatewayUrl;
  }
  await loadSyncStatus();
})();
