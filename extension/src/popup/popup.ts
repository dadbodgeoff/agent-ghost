/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { updateAlertBanner } from './components/AlertBanner';
import { updateScoreGauge } from './components/ScoreGauge';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer } from './components/SessionTimer';

const DEFAULT_SIGNALS = [0, 0, 0, 0, 0, 0, 0];

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

function levelForScore(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
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
  const scoreGauge = document.querySelector('.score-gauge') as HTMLElement | null;
  if (scoreGauge) {
    updateScoreGauge(scoreGauge, data.score, data.level);
  }

  const levelBadge = document.getElementById('levelBadge');
  if (levelBadge) {
    levelBadge.textContent = `Level ${data.level}`;
    levelBadge.className = `level-badge level-${data.level}`;
  }

  updateSignalList(data.signals);

  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    updateAlertBanner(alertEl, data.level);
  }
}

function bootstrapStaticUI(): void {
  const signalList = document.getElementById('signalList');
  if (signalList) {
    renderSignalList(signalList);
  }

  const sessionDuration = document.getElementById('sessionDuration');
  if (sessionDuration) {
    startSessionTimer(sessionDuration);
  }

  const platform = document.getElementById('platform');
  if (platform) {
    platform.textContent = 'Browser';
  }
}

bootstrapStaticUI();
updateUI({ score: 0, level: 0, signals: DEFAULT_SIGNALS });

chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (response && typeof response.score === 'number') {
    updateUI({
      score: response.score,
      level: levelForScore(response.score),
      signals: DEFAULT_SIGNALS,
    });
  }
});

(async () => {
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
})();
