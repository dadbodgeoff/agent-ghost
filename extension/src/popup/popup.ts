/**
 * Popup script — displays convergence score, signals, and gateway status.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer } from './components/SessionTimer';

const LEVEL_LABELS = [
  'Level 0 - Normal',
  'Level 1 - Soft',
  'Level 2 - Active',
  'Level 3 - Hard',
  'Level 4 - External',
];
const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'];

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

async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!el) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const ts = stored['ghost-last-sync'];
  el.textContent = typeof ts === 'number' ? new Date(ts).toLocaleTimeString() : 'never';
}

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const platformEl = document.getElementById('platform');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
  }
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] ?? `Level ${data.level}`;
    levelEl.className = `level-badge ${LEVEL_CLASSES[data.level] ?? 'level-0'}`;
  }

  updateSignalList(data.signals);

  if (platformEl && !platformEl.textContent?.trim()) {
    platformEl.textContent = 'Gateway';
  }

  if (!alertEl) {
    return;
  }

  if (data.level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      data.level >= 4
        ? 'Intervention Level 4 - External escalation active'
        : 'Intervention Level 3 - Session may be terminated';
  } else if (data.level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2 - Acknowledgment required';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) {
      return;
    }
    if (!response || typeof response.score !== 'number') {
      return;
    }

    const level =
      response.score > 0.85 ? 4 :
      response.score > 0.7 ? 3 :
      response.score > 0.5 ? 2 :
      response.score > 0.3 ? 1 : 0;

    updateUI({
      score: response.score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  });
}

(async () => {
  const signalListEl = document.getElementById('signalList');
  if (signalListEl instanceof HTMLElement) {
    renderSignalList(signalListEl);
  }

  const sessionTimerEl = document.getElementById('sessionDuration');
  if (sessionTimerEl instanceof HTMLElement) {
    startSessionTimer(sessionTimerEl);
  }

  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = auth.authenticated ? 'Gateway' : 'Offline';
  }

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
  }

  requestScore();
  await loadSyncStatus();
})();
