/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { updateAlertBanner, clearAlertBanner } from './components/AlertBanner';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer } from './components/SessionTimer';

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

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  updateSignalList(data.signals);

  if (alertEl) {
    if (data.level >= 2) {
      updateAlertBanner(alertEl, data.level);
    } else {
      clearAlertBanner(alertEl);
    }
  }
}

function updatePlatformLabel(label: string): void {
  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = label;
  }
}

function initializePopupShell(): void {
  const signalList = document.getElementById('signalList');
  const sessionDurationEl = document.getElementById('sessionDuration');

  if (signalList) {
    renderSignalList(signalList);
  }

  if (sessionDurationEl) {
    sessionDurationEl.textContent = '0h 0m 0s';
    startSessionTimer(sessionDurationEl);
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

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  initializePopupShell();

  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);
  updatePlatformLabel(auth.authenticated ? new URL(auth.gatewayUrl).hostname : 'Not connected');

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
