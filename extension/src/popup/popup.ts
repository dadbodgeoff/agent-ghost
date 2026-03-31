/**
 * Popup script — displays convergence score and signals.
 */

import { GATEWAY_URL_KEY, JWT_TOKEN_KEY } from '../background/auth-keys';
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

async function refreshGatewayState(): Promise<void> {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
    return;
  }

  const container = document.getElementById('agentList');
  if (container) {
    container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
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
    if (valueEl) valueEl.textContent = val.toFixed(3);
    if (barEl) {
      (barEl as HTMLElement).style.width = `${Math.max(0, Math.min(100, val * 100)).toFixed(0)}%`;
    }
  });

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (data.level >= 3 && alertEl) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
  } else if (alertEl) {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
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
function renderSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `Session: ${elapsed}m`;
}

renderSessionTimer();
setInterval(renderSessionTimer, 60000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  await refreshGatewayState();
  await loadSyncStatus();
})();

chrome.storage.onChanged.addListener((changes, areaName) => {
  if (areaName !== 'local') return;
  if (!changes[JWT_TOKEN_KEY] && !changes[GATEWAY_URL_KEY] && !changes['ghost-last-sync']) return;

  if (changes[JWT_TOKEN_KEY] || changes[GATEWAY_URL_KEY]) {
    void refreshGatewayState();
  }

  if (changes['ghost-last-sync']) {
    void loadSyncStatus();
  }
});
