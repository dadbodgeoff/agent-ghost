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

function setAgentListMessage(message: string): void {
  const container = document.getElementById('agentList');
  if (!container) return;
  container.replaceChildren();
  const item = document.createElement('span');
  item.className = 'agent-list-empty';
  item.textContent = message;
  container.append(item);
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
      setAgentListMessage('No agents found');
      return;
    }
    const items = agents.map((agent) => {
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
    });
    container.replaceChildren(...items);
  } catch {
    setAgentListMessage('Unable to load agents');
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
  const scoreEl = document.getElementById('score');
  const levelEl = document.getElementById('level');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level level-${data.level}`;
  }

  const signalIds = ['s1', 's2', 's3', 's4', 's5', 's6', 's7'];
  data.signals.forEach((val, i) => {
    const el = document.getElementById(signalIds[i]);
    if (el) el.textContent = val.toFixed(2);
  });

  // Alert banner
  const alertEl = document.getElementById('alert');
  const alertText = document.getElementById('alert-text');
  if (data.level >= 3 && alertEl && alertText) {
    alertEl.classList.add('visible');
    alertText.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
  } else if (alertEl && alertText) {
    alertEl.classList.remove('visible');
    alertText.textContent = '';
  }
}

// Request score from background
chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (chrome.runtime.lastError) {
    updateConnectionIndicator(false);
    return;
  }
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
  const timerEl = document.getElementById('timer');
  if (timerEl) timerEl.textContent = `Session: ${elapsed}m`;
}
updateSessionTimer();
setInterval(updateSessionTimer, 60000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    setAgentListMessage('Not connected to gateway');
  }

  await loadSyncStatus();
})();
