/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
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

function renderSignals(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map((name, index) => {
    const value = signals[index] ?? 0;
    const width = Math.max(0, Math.min(100, value * 100));

    return `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value">${value.toFixed(3)}</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" style="width:${width.toFixed(0)}%;background:${scoreColor(value)}"></div>
        </div>
      </div>
    `;
  }).join('');
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

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignals(data.signals);

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (data.level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
  } else if (data.level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Elevated convergence detected. Monitor session closely.';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') {
      return;
    }

    const level = response.score > 0.85 ? 4 :
      response.score > 0.7 ? 3 :
      response.score > 0.5 ? 2 :
      response.score > 0.3 ? 1 : 0;

    updateUI({
      score: response.score,
      level,
      signals: Array.from({ length: SIGNAL_NAMES.length }, () => 0),
    });
  });
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

// Session timer
const sessionStart = Date.now();
function updateSessionTimer(): void {
  const elapsedSeconds = Math.floor((Date.now() - sessionStart) / 1000);
  const hours = Math.floor(elapsedSeconds / 3600);
  const minutes = Math.floor((elapsedSeconds % 3600) / 60);
  const seconds = elapsedSeconds % 60;
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
}

document.addEventListener('DOMContentLoaded', async () => {
  renderSignals(Array.from({ length: SIGNAL_NAMES.length }, () => 0));
  updateSessionTimer();
  setInterval(updateSessionTimer, 1000);
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
});
