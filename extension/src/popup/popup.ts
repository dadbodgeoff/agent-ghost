/**
 * Popup script — displays convergence score, sync state, and connected agents.
 */

import { initAuthSync } from '../background/auth-sync';
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
    dot.setAttribute('aria-label', connected ? 'Connected' : 'Disconnected');
  }
  if (label) {
    label.classList.remove('connected', 'disconnected');
    label.classList.add(connected ? 'connected' : 'disconnected');
    label.textContent = connected ? 'Connected' : 'Disconnected';
  }
}

function showAgentListMessage(message: string): void {
  const container = document.getElementById('agentList');
  if (!container) return;

  container.replaceChildren();
  const messageEl = document.createElement('span');
  messageEl.className = 'agent-list-empty';
  messageEl.textContent = message;
  container.appendChild(messageEl);
}

function renderAgentList(agents: Array<{ id: string; name: string; state: string }>): void {
  const container = document.getElementById('agentList');
  if (!container) return;

  container.replaceChildren();
  for (const agent of agents) {
    const item = document.createElement('div');
    item.className = 'agent-list-item';

    const name = document.createElement('span');
    name.className = 'agent-name';
    name.textContent = agent.name || agent.id;

    const state = document.createElement('span');
    state.className = 'agent-state';
    state.textContent = agent.state;

    item.append(name, state);
    container.appendChild(item);
  }
}

function updateAlertBanner(level: number): void {
  const banner = document.getElementById('alertBanner');
  if (!banner) return;

  if (level >= 3) {
    banner.className = 'alert-banner active alert-danger';
    banner.textContent = `Intervention Level ${level} - Consider ending the session`;
    return;
  }

  if (level >= 2) {
    banner.className = 'alert-banner active alert-warning';
    banner.textContent = 'Intervention Level 2 - Caution advised';
    return;
  }

  banner.className = 'alert-banner';
  banner.textContent = '';
}

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.replaceChildren();
  for (const [index, name] of SIGNAL_NAMES.entries()) {
    const row = document.createElement('div');
    row.className = 'signal-row';

    const label = document.createElement('span');
    label.className = 'signal-name';
    label.textContent = name;

    const value = document.createElement('span');
    value.className = 'signal-value';
    value.id = `signal-value-${index}`;
    value.textContent = '0.000';

    const bar = document.createElement('div');
    bar.className = 'signal-bar';

    const fill = document.createElement('div');
    fill.className = 'signal-bar-fill';
    fill.id = `signal-bar-${index}`;
    fill.style.width = '0%';

    bar.appendChild(fill);
    row.append(label, value, bar);
    container.appendChild(row);
  }
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function updateUI(data: { score: number; level: number; signals: number[]; platform?: string }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelBadge = document.getElementById('levelBadge');
  const platform = document.getElementById('platform');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.score);
  }

  if (levelBadge) {
    levelBadge.textContent = LEVEL_LABELS[data.level] ?? `Level ${data.level}`;
    levelBadge.className = `level-badge ${LEVEL_CLASSES[data.level] ?? 'level-0'}`;
  }

  for (const [index, value] of data.signals.entries()) {
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) {
      valueEl.textContent = value.toFixed(3);
    }
    if (barEl) {
      (barEl as HTMLElement).style.width = `${Math.max(0, Math.min(1, value)) * 100}%`;
      (barEl as HTMLElement).style.background = scoreColor(value);
    }
  }

  if (platform && data.platform) {
    platform.textContent = data.platform;
  }

  updateAlertBanner(data.level);
}

async function loadAgentList(): Promise<void> {
  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      showAgentListMessage('No agents found');
      return;
    }
    renderAgentList(agents);
    updateConnectionIndicator(true);
  } catch {
    updateConnectionIndicator(false);
    showAgentListMessage('Unable to load agents');
  }
}

async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!el) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const timestamp = stored['ghost-last-sync'];
  el.textContent =
    typeof timestamp === 'number' ? new Date(timestamp).toLocaleTimeString() : 'never';
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response: { score?: number } | undefined) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') {
      updateConnectionIndicator(false);
      return;
    }

    const score = response.score;
    const level = score > 0.85 ? 4 : score > 0.7 ? 3 : score > 0.5 ? 2 : score > 0.3 ? 1 : 0;
    updateUI({
      score,
      level,
      signals: [score, 0, 0, 0, 0, 0, 0],
    });
  });
}

function startSessionTimer(): void {
  const el = document.getElementById('sessionDuration');
  if (!el) return;

  const startedAt = Date.now();
  const renderElapsed = () => {
    const elapsed = Math.floor((Date.now() - startedAt) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    el.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  renderElapsed();
  window.setInterval(renderElapsed, 1000);
}

async function initializePopup(): Promise<void> {
  renderSignalList();
  startSessionTimer();
  requestScore();

  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    showAgentListMessage('Not connected to gateway');
  }

  await loadSyncStatus();
}

document.addEventListener('DOMContentLoaded', () => {
  void initializePopup();
});
