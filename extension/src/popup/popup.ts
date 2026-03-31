/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
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

let sessionStart = Date.now();

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map(
    (name, i) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${i}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${i}" style="width:0%"></div>
        </div>
      </div>
    `
  ).join('');
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
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

function updateUI(data: {
  composite_score?: number;
  score?: number;
  level?: number;
  signals?: number[];
  platform?: string;
}): void {
  const score = typeof data.composite_score === 'number'
    ? data.composite_score
    : typeof data.score === 'number'
      ? data.score
      : 0;
  const level = typeof data.level === 'number' ? data.level : 0;
  const signals = Array.isArray(data.signals) ? data.signals : [0, 0, 0, 0, 0, 0, 0];

  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(score);
  }
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[level] ?? `Level ${level}`;
    levelEl.className = `level-badge level-${level}`;
  }

  signals.forEach((val, i) => {
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`) as HTMLElement | null;
    if (valueEl) valueEl.textContent = val.toFixed(3);
    if (barEl) {
      barEl.style.width = `${Math.max(0, Math.min(val, 1)) * 100}%`;
      barEl.style.background = scoreColor(val);
    }
  });

  const platformEl = document.getElementById('platform');
  if (platformEl && data.platform) {
    platformEl.textContent = data.platform;
  }

  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      level === 4
        ? 'Intervention Level 4 - External escalation active'
        : `Intervention Level ${level} - Session may be terminated`;
  } else if (level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2 - Acknowledgment required';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

function startSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;

  setInterval(() => {
    const nextElapsed = Math.floor((Date.now() - sessionStart) / 60000);
    const timerNode = document.getElementById('sessionDuration');
    if (timerNode) timerNode.textContent = `${nextElapsed}m`;
  }, 60_000);
}

function requestStatus(): void {
  chrome.runtime.sendMessage({ type: 'get_status' }, (response) => {
    if (chrome.runtime.lastError || !response) {
      return;
    }

    updateConnectionIndicator(Boolean(response.connected));
    if (response.latestScore) {
      updateUI(response.latestScore);
    }
  });
}

async function initPopup(): Promise<void> {
  renderSignalList();
  startSessionTimer();
  requestStatus();

  chrome.runtime.onMessage.addListener((message) => {
    if (message?.type === 'score_update' && message.data) {
      updateUI(message.data);
    }
  });

  const auth = await initAuthSync();
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
}

document.addEventListener('DOMContentLoaded', () => {
  void initPopup();
});
