/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

interface PopupStatus {
  score: number;
  platform?: string | null;
  sessionId?: string | null;
  pageUrl?: string | null;
  updatedAt?: string | null;
}

function clearNode(node: Element): void {
  while (node.firstChild) {
    node.removeChild(node.firstChild);
  }
}

function appendTextElement(parent: Element, className: string, text: string): void {
  const span = document.createElement('span');
  span.className = className;
  span.textContent = text;
  parent.appendChild(span);
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

function renderAgentList(
  container: HTMLElement,
  agents: Array<{ id: string; name: string; state: string }>,
): void {
  clearNode(container);

  if (agents.length === 0) {
    const empty = document.createElement('span');
    empty.className = 'agent-list-empty';
    empty.textContent = 'No agents found';
    container.appendChild(empty);
    return;
  }

  for (const agent of agents) {
    const row = document.createElement('div');
    row.className = 'agent-list-item';
    appendTextElement(row, 'agent-name', agent.name || agent.id);
    appendTextElement(row, 'agent-state', agent.state);
    container.appendChild(row);
  }
}

function renderAgentListMessage(container: HTMLElement, message: string): void {
  clearNode(container);
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = message;
  container.appendChild(empty);
}

function normalizePlatformLabel(platform: string | null | undefined): string {
  if (!platform) {
    return 'Unknown';
  }

  return platform
    .split(/[-_]/g)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

/**
 * Fetch and render the agent list from the gateway.
 */
async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!(container instanceof HTMLElement)) return;

  try {
    const agents = await getAgents();
    renderAgentList(container, agents);
  } catch {
    renderAgentListMessage(container, 'Unable to load agents');
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

function renderSignals(signals: number[]): void {
  const list = document.getElementById('signalList');
  if (!(list instanceof HTMLElement)) return;

  clearNode(list);
  signals.forEach((value, index) => {
    const row = document.createElement('div');
    row.className = 'signal-row';

    appendTextElement(row, 'signal-name', `Signal ${index + 1}`);
    appendTextElement(row, 'signal-value', value.toFixed(2));

    const bar = document.createElement('div');
    bar.className = 'signal-bar';
    const fill = document.createElement('div');
    fill.className = 'signal-bar-fill';
    fill.style.width = `${Math.max(0, Math.min(100, value * 100))}%`;
    fill.style.background = value >= 0.7 ? '#ef4444' : value >= 0.4 ? '#f59e0b' : '#22c55e';
    bar.appendChild(fill);
    row.appendChild(bar);
    list.appendChild(row);
  });
}

function updateUI(data: { score: number; level: number; signals: number[]; platform?: string | null }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignals(data.signals);

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = normalizePlatformLabel(data.platform);
  }

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    alertEl.classList.remove('active', 'alert-warning', 'alert-danger');
    alertEl.textContent = '';
    if (data.level >= 4) {
      alertEl.classList.add('active', 'alert-danger');
      alertEl.textContent = 'Convergence level 4 detected. End the session and review signals.';
    } else if (data.level >= 3) {
      alertEl.classList.add('active', 'alert-warning');
      alertEl.textContent = 'Convergence level 3 detected. Consider taking a break.';
    }
  }
}

function getLevel(score: number): number {
  return score > 0.85 ? 4 :
    score > 0.7 ? 3 :
    score > 0.5 ? 2 :
    score > 0.3 ? 1 : 0;
}

function updateSessionDuration(startedAt: number): void {
  const elapsed = Math.floor((Date.now() - startedAt) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response?: PopupStatus) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') {
      return;
    }

    updateUI({
      score: response.score,
      level: getLevel(response.score),
      platform: response.platform,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  });
}

const sessionStart = Date.now();
updateSessionDuration(sessionStart);
setInterval(() => {
  updateSessionDuration(sessionStart);
}, 60000);
requestScore();
setInterval(requestScore, 30000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container instanceof HTMLElement) {
      renderAgentListMessage(container, 'Not connected to gateway');
    }
  }

  await loadSyncStatus();
})();
