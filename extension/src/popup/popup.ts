/**
 * Popup script — displays convergence score, status, and agent connectivity.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_LABELS = [
  'Velocity',
  'Context',
  'Interruptions',
  'Confidence',
  'Corrections',
  'Drift',
  'Escalation',
];

function scoreToLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
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

  container.innerHTML = '<span class="agent-list-empty">Loading agents...</span>';

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

  try {
    const stored = await chrome.storage.local.get('ghost-last-sync');
    const ts = stored['ghost-last-sync'];
    if (typeof ts === 'number') {
      el.textContent = new Date(ts).toLocaleTimeString();
    } else {
      el.textContent = 'never';
    }
  } catch {
    el.textContent = 'unknown';
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

  const signalList = document.getElementById('signalList');
  if (signalList) {
    signalList.innerHTML = data.signals
      .map((value, index) => {
        const width = Math.max(0, Math.min(100, value * 100));
        const color = value > 0.7 ? '#ef4444' : value > 0.4 ? '#f59e0b' : '#22c55e';
        return (
          `<div class="signal-row">` +
          `<span class="signal-name">${SIGNAL_LABELS[index] ?? `Signal ${index + 1}`}</span>` +
          `<span style="display:flex;align-items:center;">` +
          `<span class="signal-value">${value.toFixed(2)}</span>` +
          `<span class="signal-bar" aria-hidden="true">` +
          `<span class="signal-bar-fill" style="width:${width}%;background:${color};"></span>` +
          `</span>` +
          `</span>` +
          `</div>`
        );
      })
      .join('');
  }

  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = `alert-banner active ${data.level >= 4 ? 'alert-danger' : 'alert-warning'}`;
      alertEl.textContent =
        data.level >= 4
          ? 'High convergence detected. Step away or rotate tasks now.'
          : `Convergence level ${data.level} detected. Consider taking a break.`;
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function loadScore(): Promise<void> {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
      const score = typeof response?.score === 'number' ? response.score : 0;
      updateUI({
        score,
        level: scoreToLevel(score),
        signals: [0, 0, 0, 0, 0, 0, 0],
      });
      resolve();
    });
  });
}

function updateSessionDuration(sessionStart: number): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

function updatePlatformLabel(): void {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;
  const ua = navigator.userAgent.toLowerCase();
  platformEl.textContent = ua.includes('firefox')
    ? 'Firefox extension'
    : ua.includes('edg/')
      ? 'Edge extension'
      : 'Chrome extension';
}

(async () => {
  const sessionStart = Date.now();
  updateSessionDuration(sessionStart);
  setInterval(() => updateSessionDuration(sessionStart), 60000);
  updatePlatformLabel();

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

  await Promise.all([loadScore(), loadSyncStatus()]);
})();
