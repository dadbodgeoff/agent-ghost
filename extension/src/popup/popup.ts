/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_IDS = ['signal-value-0', 'signal-value-1', 'signal-value-2', 'signal-value-3', 'signal-value-4', 'signal-value-5', 'signal-value-6'];

function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return `${hours}h ${minutes}m ${seconds}s`;
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
    container.replaceChildren(
      ...agents.map((agent) => {
        const row = document.createElement('div');
        row.className = 'agent-list-item';

        const name = document.createElement('span');
        name.className = 'agent-name';
        name.textContent = agent.name || agent.id;

        const state = document.createElement('span');
        state.className = 'agent-state';
        state.textContent = agent.state;

        row.append(name, state);
        return row;
      }),
    );
  } catch {
    updateConnectionIndicator(false);
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

  SIGNAL_IDS.forEach((id, i) => {
    const el = document.getElementById(id);
    const value = data.signals[i] ?? 0;
    if (el) el.textContent = value.toFixed(3);
  });

  data.signals.forEach((value, i) => {
    const bar = document.getElementById(`signal-bar-${i}`);
    if (bar instanceof HTMLElement) {
      bar.style.width = `${Math.max(0, Math.min(100, value * 100))}%`;
    }
  });

  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (data.level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
  } else if (data.level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Convergence level 2 detected. Stay aware of session drift.';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

function updateSessionTimer(sessionStart: number): void {
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) {
    timerEl.textContent = formatDuration(Date.now() - sessionStart);
  }
}

function loadScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') {
      updateConnectionIndicator(false);
      return;
    }

    const level = response.score > 0.85 ? 4 :
      response.score > 0.7 ? 3 :
      response.score > 0.5 ? 2 :
      response.score > 0.3 ? 1 : 0;
    updateConnectionIndicator(true);
    updateUI({
      score: response.score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  });
}

async function initPopup(): Promise<void> {
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
}

document.addEventListener('DOMContentLoaded', () => {
  loadScore();

  const sessionStart = Date.now();
  updateSessionTimer(sessionStart);
  setInterval(() => {
    updateSessionTimer(sessionStart);
  }, 1000);

  void initPopup();
});
