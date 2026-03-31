/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { renderSignalList, updateSignalList } from './components/SignalList';

interface AgentListEntry {
  id?: string;
  name?: string;
  state?: string;
}

function setText(id: string, text: string): HTMLElement | null {
  const element = document.getElementById(id);
  if (element) {
    element.textContent = text;
  }
  return element;
}

function renderAgentEmpty(container: HTMLElement, text: string): void {
  container.replaceChildren();
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = text;
  container.appendChild(empty);
}

function renderAgentList(container: HTMLElement, agents: AgentListEntry[]): void {
  container.replaceChildren();

  for (const agent of agents) {
    const item = document.createElement('div');
    item.className = 'agent-list-item';

    const name = document.createElement('span');
    name.className = 'agent-name';
    name.textContent = agent.name || agent.id || 'Unnamed agent';

    const state = document.createElement('span');
    state.className = 'agent-state';
    state.textContent = agent.state || 'unknown';

    item.append(name, state);
    container.appendChild(item);
  }
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
    const agents = (await getAgents()) as AgentListEntry[];
    if (agents.length === 0) {
      renderAgentEmpty(container, 'No agents found');
      return;
    }
    renderAgentList(container, agents);
  } catch {
    renderAgentEmpty(container, 'Unable to load agents');
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

  updateSignalList(data.signals);

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (data.level >= 3 && alertEl) {
    alertEl.classList.add('visible');
    alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
  } else if (alertEl) {
    alertEl.classList.remove('visible');
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
setText('sessionDuration', 'Session: 0m');
setInterval(() => {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  setText('sessionDuration', `Session: ${elapsed}m`);
}, 60000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  const signalList = document.getElementById('signalList');
  if (signalList) {
    renderSignalList(signalList);
  }

  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      renderAgentEmpty(container, 'Not connected to gateway');
    }
  }

  await loadSyncStatus();
})();
