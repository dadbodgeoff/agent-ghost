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

function updatePlatformLabel(value: string): void {
  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = value;
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

  const signalsEl = document.getElementById('signalList');
  if (signalsEl) {
    const labels = [
      'Cadence',
      'Escalation',
      'Repetition',
      'Breadth',
      'Confidence',
      'Corrections',
      'Volatility',
    ];
    signalsEl.innerHTML = data.signals
      .map((value, index) => {
        const clamped = Math.max(0, Math.min(1, value));
        return (
          `<div class="signal-row">` +
          `<span class="signal-name">${labels[index] ?? `Signal ${index + 1}`}</span>` +
          `<span class="signal-value">${value.toFixed(2)}</span>` +
          `<div class="signal-bar" aria-hidden="true">` +
          `<div class="signal-bar-fill" style="width: ${clamped * 100}%; background: ${data.level >= 3 ? '#f87171' : '#22c55e'};"></div>` +
          `</div>` +
          `</div>`
        );
      })
      .join('');
  }

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
function updateSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

updateSessionTimer();
setInterval(updateSessionTimer, 60000);

// Hydrate auth from extension storage in the popup context before reading gateway-backed UI.
(async () => {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  updatePlatformLabel(new URL(auth.gatewayUrl).host);

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
