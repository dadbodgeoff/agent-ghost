/**
 * Popup script — displays convergence score, connection state, and sync status.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
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

function updateConnectionIndicator(connected: boolean): void {
  const dot = document.getElementById('statusDot');
  const label = document.getElementById('statusLabel');
  if (dot) {
    dot.className = `status-dot ${connected ? 'connected' : 'disconnected'}`;
    dot.setAttribute('aria-label', connected ? 'Connected' : 'Disconnected');
  }
  if (label) {
    label.className = `status-label ${connected ? 'connected' : 'disconnected'}`;
    label.textContent = connected ? 'Connected' : 'Disconnected';
  }
}

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map(
    (name, index) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${index}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${index}" style="width:0%"></div>
        </div>
      </div>
    `,
  ).join('');
}

function getLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function updateScore(score: number): void {
  const level = getLevel(score);
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
    scoreEl.setAttribute('style', `color: ${scoreColor(score)}`);
  }

  if (levelEl) {
    levelEl.textContent = `Level ${level}`;
    levelEl.className = `level-badge level-${level}`;
  }

  for (let index = 0; index < SIGNAL_NAMES.length; index += 1) {
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) {
      valueEl.textContent = '0.000';
    }
    if (barEl) {
      barEl.setAttribute('style', 'width: 0%;');
    }
  }

  if (!alertEl) return;
  if (level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent = `Convergence level ${level} detected. Consider taking a break.`;
  } else if (level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Convergence level 2 detected. Review the session before continuing.';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
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
        (agent) => `
          <div class="agent-list-item">
            <span class="agent-name">${agent.name || agent.id}</span>
            <span class="agent-state">${agent.state}</span>
          </div>
        `,
      )
      .join('');
  } catch {
    container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
  }
}

async function loadSyncStatus(): Promise<void> {
  const statusEl = document.getElementById('syncStatus');
  if (!statusEl) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const lastSync = stored['ghost-last-sync'];
  statusEl.textContent =
    typeof lastSync === 'number' ? new Date(lastSync).toLocaleTimeString() : 'never';
}

function startSessionTimer(): void {
  const sessionStart = Date.now();
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const tick = () => {
    const elapsed = Math.floor((Date.now() - sessionStart) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  tick();
  setInterval(tick, 1000);
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response?: { score?: number }) => {
    if (chrome.runtime.lastError || typeof response?.score !== 'number') {
      return;
    }
    updateScore(response.score);
  });
}

async function initPopup(): Promise<void> {
  renderSignalList();
  startSessionTimer();
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

  await loadSyncStatus();
  requestScore();

  chrome.runtime.onMessage.addListener((message) => {
    if (message?.type === 'score_update' && typeof message.score === 'number') {
      updateScore(message.score);
    }
  });
}

void initPopup();
