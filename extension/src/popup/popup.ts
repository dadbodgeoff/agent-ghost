/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState } from '../background/auth-sync';
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

const LEVEL_LABELS = [
  'Level 0 - Normal',
  'Level 1 - Soft',
  'Level 2 - Active',
  'Level 3 - Hard',
  'Level 4 - External',
];

const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'];

let sessionStart = Date.now();

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
      container.replaceChildren(createMessage('No agents found'));
      return;
    }

    const rows = agents.map((agent) => {
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
    });

    container.replaceChildren(...rows);
  } catch {
    container.replaceChildren(createMessage('Unable to load agents'));
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

function createMessage(text: string): HTMLSpanElement {
  const el = document.createElement('span');
  el.className = 'agent-list-empty';
  el.textContent = text;
  return el;
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  const rows = SIGNAL_NAMES.map((name, index) => {
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
    return row;
  });

  container.replaceChildren(...rows);
}

function startSessionTimer(): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const render = () => {
    const elapsed = Math.floor((Date.now() - sessionStart) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  render();
  window.setInterval(render, 1000);
}

function updateUI(data: { score: number; level: number; signals: number[]; platform?: string }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.score);
  }
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] ?? `Level ${data.level}`;
    levelEl.className = `level-badge ${LEVEL_CLASSES[data.level] ?? 'level-0'}`;
  }

  data.signals.forEach((val, index) => {
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) valueEl.textContent = val.toFixed(3);
    if (barEl instanceof HTMLElement) {
      barEl.style.width = `${Math.max(0, Math.min(100, val * 100)).toFixed(0)}%`;
      barEl.style.background = scoreColor(val);
    }
  });

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = data.platform ?? 'Browser session';
  }

  const alertEl = document.getElementById('alertBanner');
  if (!(alertEl instanceof HTMLElement)) return;

  if (data.level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      data.level === 4
        ? 'Intervention Level 4 - External escalation active'
        : `Intervention Level ${data.level} - Session may be terminated`;
  } else if (data.level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2 - Acknowledgment required';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || response.score === undefined) {
      return;
    }

    const level =
      response.score > 0.85 ? 4 :
      response.score > 0.7 ? 3 :
      response.score > 0.5 ? 2 :
      response.score > 0.3 ? 1 : 0;

    updateUI({
      score: response.score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  });
}

document.addEventListener('DOMContentLoaded', async () => {
  renderSignalList();
  startSessionTimer();
  requestScore();

  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.replaceChildren(createMessage('Not connected to gateway'));
    }
  }

  await loadSyncStatus();
});
