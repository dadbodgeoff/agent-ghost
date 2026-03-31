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
          `</div>`,
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

  data.signals.forEach((val, i) => {
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`);
    if (valueEl) {
      valueEl.textContent = val.toFixed(3);
    }
    if (barEl instanceof HTMLElement) {
      barEl.style.width = `${Math.max(0, Math.min(1, val)) * 100}%`;
    }
  });

  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.classList.add('active', 'alert-danger');
      alertEl.classList.remove('alert-warning');
      alertEl.textContent = `Intervention Level ${data.level} — Session may be terminated`;
    } else if (data.level >= 2) {
      alertEl.classList.add('active', 'alert-warning');
      alertEl.classList.remove('alert-danger');
      alertEl.textContent = 'Intervention Level 2 — Acknowledgment required';
    } else {
      alertEl.classList.remove('active', 'alert-warning', 'alert-danger');
      alertEl.textContent = '';
    }
  }
}

function renderScore(score: number): void {
  const level = score > 0.85 ? 4 :
                score > 0.7 ? 3 :
                score > 0.5 ? 2 :
                score > 0.3 ? 1 : 0;
  updateUI({
    score,
    level,
    signals: [0, 0, 0, 0, 0, 0, 0],
  });
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    const score = typeof response?.score === 'number' ? response.score : 0;
    renderScore(score);
  });
}

chrome.runtime.onMessage.addListener((message) => {
  if (message?.type === 'score_update' && typeof message.data?.score === 'number') {
    renderScore(message.data.score);
  }
});

requestScore();
setInterval(requestScore, 5000);

const sessionStart = Date.now();
function renderSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}
renderSessionTimer();
setInterval(renderSessionTimer, 60000);

(async () => {
  const auth = await initAuthSync();
  const platformEl = document.getElementById('platform');
  if (platformEl) {
    try {
      platformEl.textContent = new URL(auth.gatewayUrl).host;
    } catch {
      platformEl.textContent = auth.gatewayUrl;
    }
  }
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
  }

  const syncStatus = document.getElementById('syncStatus');
  if (syncStatus && auth.lastValidated > 0) {
    syncStatus.textContent = new Date(auth.lastValidated).toLocaleTimeString();
    return;
  }

  await loadSyncStatus();
})();
