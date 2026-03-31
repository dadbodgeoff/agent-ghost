/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents, getHealth, getScores, type AgentSummary } from '../background/gateway-client';
import { updateAlertBanner } from './components/AlertBanner';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer, stopSessionTimer } from './components/SessionTimer';

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

function setAgentListMessage(container: HTMLElement, message: string): void {
  container.textContent = '';
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = message;
  container.append(empty);
}

function renderAgentList(container: HTMLElement, agents: AgentSummary[]): void {
  container.textContent = '';

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
    container.append(item);
  }
}

async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!(container instanceof HTMLElement)) return;

  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      setAgentListMessage(container, 'No agents found');
      return;
    }
    renderAgentList(container, agents);
  } catch {
    setAgentListMessage(container, 'Unable to load agents');
  }
}

async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!(el instanceof HTMLElement)) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const ts = stored['ghost-last-sync'];
  if (ts && typeof ts === 'number') {
    el.textContent = new Date(ts).toLocaleTimeString();
  } else {
    el.textContent = 'never';
  }
}

function updatePlatformLabel(platform: string | null | undefined): void {
  const platformEl = document.getElementById('platform');
  if (platformEl instanceof HTMLElement) {
    platformEl.textContent = platform || 'unknown';
  }
}

function updateUI(data: { score: number; level: number; signals: number[]; platform?: string }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  updateSignalList(data.signals);
  updatePlatformLabel(data.platform);

  if (alertEl instanceof HTMLElement) {
    updateAlertBanner(alertEl, data.level);
  }
}

function scoreToLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

async function loadScoreData(): Promise<void> {
  try {
    const snapshots = await getScores();
    const snapshot = snapshots[0];
    if (snapshot) {
      updateUI({
        score: snapshot.score,
        level: snapshot.level,
        signals: snapshot.signals ?? [0, 0, 0, 0, 0, 0, 0],
        platform: snapshot.platform,
      });
      return;
    }
  } catch {
    // Fall back to the native-messaging score when the gateway score API is unavailable.
  }

  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) {
      return;
    }

    if (response && typeof response.score === 'number') {
      updateUI({
        score: response.score,
        level: scoreToLevel(response.score),
        signals: [0, 0, 0, 0, 0, 0, 0],
      });
    }
  });
}

(async () => {
  const signalList = document.getElementById('signalList');
  if (signalList instanceof HTMLElement) {
    renderSignalList(signalList);
  }

  const sessionDuration = document.getElementById('sessionDuration');
  if (sessionDuration instanceof HTMLElement) {
    startSessionTimer(sessionDuration);
    window.addEventListener('unload', () => stopSessionTimer(), { once: true });
  }

  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  await loadScoreData();

  if (auth.authenticated) {
    try {
      await getHealth();
    } catch {
      updateConnectionIndicator(false);
    }
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container instanceof HTMLElement) {
      setAgentListMessage(container, 'Not connected to gateway');
    }
  }

  await loadSyncStatus();
})();
