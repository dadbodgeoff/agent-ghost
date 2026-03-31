/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const LEVEL_LABELS = [
  'Level 0',
  'Level 1',
  'Level 2',
  'Level 3',
  'Level 4',
];
const SIGNAL_NAMES = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
];

function levelClass(level: number): string {
  return `level-${Math.max(0, Math.min(4, level))}`;
}

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

function renderSignalList(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map((name, index) => {
    const value = signals[index] ?? 0;
    return [
      '<div class="signal-row">',
      `<span class="signal-name">${name}</span>`,
      `<span class="signal-value">${value.toFixed(2)}</span>`,
      '<div class="signal-bar">',
      `<div class="signal-bar-fill ${levelClass(Math.round(value * 4))}" style="width:${Math.max(0, Math.min(100, value * 100))}%"></div>`,
      '</div>',
      '</div>',
    ].join('');
  }).join('');
}

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] ?? `Level ${data.level}`;
    levelEl.className = `level-badge ${levelClass(data.level)}`;
  }

  renderSignalList(data.signals);

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = 'alert-banner active alert-danger';
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else if (data.level >= 2) {
      alertEl.className = 'alert-banner active alert-warning';
      alertEl.textContent = 'Convergence is elevated. Stay aware of session boundaries.';
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || response.score === undefined) {
      return;
    }

    const score = typeof response.score === 'number' ? response.score : 0;
    const level = score > 0.85 ? 4
      : score > 0.7 ? 3
      : score > 0.5 ? 2
      : score > 0.3 ? 1
      : 0;

    updateUI({
      score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  });
}

// Session timer
const sessionStart = Date.now();
function updateSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  updateUI({ score: 0, level: 0, signals: [0, 0, 0, 0, 0, 0, 0] });
  updateSessionTimer();
  setInterval(updateSessionTimer, 60000);
  requestScore();

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

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = auth.authenticated ? 'Gateway' : 'Offline';
  }

  await loadSyncStatus();
})();
