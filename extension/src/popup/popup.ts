/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { updateAlertBanner } from './components/AlertBanner';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer } from './components/SessionTimer';

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

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

function setPlatformLabel(value: string): void {
  const element = document.getElementById('platform');
  if (element) {
    element.textContent = value;
  }
}

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
        (agent) =>
          `<div class="agent-list-item">` +
          `<span class="agent-name">${escapeHtml(agent.name || agent.id)}</span>` +
          `<span class="agent-state">${escapeHtml(agent.state)}</span>` +
          `</div>`
      )
      .join('');
  } catch {
    container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
  }
}

async function loadSyncStatus(): Promise<void> {
  const element = document.getElementById('syncStatus');
  if (!element) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const timestamp = stored['ghost-last-sync'];
  element.textContent =
    timestamp && typeof timestamp === 'number'
      ? new Date(timestamp).toLocaleTimeString()
      : 'never';
}

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
  }

  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  updateSignalList(data.signals);

  if (alertEl) {
    updateAlertBanner(alertEl, data.level);
  }
}

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

(async () => {
  const signalList = document.getElementById('signalList');
  if (signalList) {
    renderSignalList(signalList);
  }

  const sessionDuration = document.getElementById('sessionDuration');
  if (sessionDuration) {
    startSessionTimer(sessionDuration);
  }

  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);
  setPlatformLabel(auth.authenticated ? new URL(auth.gatewayUrl).hostname : 'Offline');

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
