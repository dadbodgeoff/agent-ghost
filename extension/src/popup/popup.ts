/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const DEFAULT_SIGNALS = [0, 0, 0, 0, 0, 0, 0];

function computeLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

function setTextContent(id: string, value: string): void {
  const element = document.getElementById(id);
  if (element) {
    element.textContent = value;
  }
}

function renderSignals(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  const signalNames = [
    'Session Duration',
    'Inter-Session Gap',
    'Response Latency',
    'Vocabulary Convergence',
    'Goal Boundary Erosion',
    'Initiative Balance',
    'Disengagement Resistance',
  ];

  container.innerHTML = signalNames
    .map((name, index) => {
      const value = signals[index] ?? 0;
      const color =
        value < 0.3 ? '#22c55e' : value < 0.5 ? '#eab308' : value < 0.7 ? '#f97316' : '#ef4444';

      return `
        <div class="signal-row" role="listitem">
          <span class="signal-name">${name}</span>
          <span class="signal-value">${value.toFixed(3)}</span>
          <div class="signal-bar" aria-hidden="true">
            <div class="signal-bar-fill" style="width:${Math.round(value * 100)}%;background:${color}"></div>
          </div>
        </div>
      `;
    })
    .join('');
}

function updateAlert(level: number): void {
  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (level >= 4) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent = 'Intervention Level 4: external escalation active.';
    return;
  }

  if (level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent = 'Intervention Level 3: session may be terminated.';
    return;
  }

  if (level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2: acknowledgment recommended.';
    return;
  }

  alertEl.className = 'alert-banner';
  alertEl.textContent = '';
}

function updateSessionDuration(startedAt: number): void {
  const elapsedSeconds = Math.floor((Date.now() - startedAt) / 1000);
  const hours = Math.floor(elapsedSeconds / 3600);
  const minutes = Math.floor((elapsedSeconds % 3600) / 60);
  const seconds = elapsedSeconds % 60;
  setTextContent('sessionDuration', `${hours}h ${minutes}m ${seconds}s`);
}

function setPlatformLabel(): void {
  const ua = navigator.userAgent.toLowerCase();
  const label =
    ua.includes('firefox') ? 'Firefox extension' :
    ua.includes('edg/') ? 'Edge extension' :
    ua.includes('chrome') ? 'Chrome extension' :
    'Browser extension';

  setTextContent('platform', label);
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
  updateAlert(data.level);
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) {
      updateUI({
        score: 0,
        level: 0,
        signals: DEFAULT_SIGNALS,
      });
      return;
    }

    if (response && typeof response.score === 'number') {
      updateUI({
        score: response.score,
        level: computeLevel(response.score),
        signals: DEFAULT_SIGNALS,
      });
      return;
    }

    updateUI({
      score: 0,
      level: 0,
      signals: DEFAULT_SIGNALS,
    });
  });
}

const sessionStart = Date.now();

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  renderSignals(DEFAULT_SIGNALS);
  setPlatformLabel();
  updateSessionDuration(sessionStart);
  setInterval(() => updateSessionDuration(sessionStart), 1000);

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

  requestScore();
  await loadSyncStatus();
})();
