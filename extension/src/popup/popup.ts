/**
 * Popup script — renders the built extension popup UI.
 */

import { getAuthState } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { updateAlertBanner } from './components/AlertBanner';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer, stopSessionTimer } from './components/SessionTimer';

type PopupScorePayload = {
  score: number;
  level?: number;
  signals?: number[];
  platform?: string;
};

const EMPTY_SIGNALS = [0, 0, 0, 0, 0, 0, 0];

function deriveLevel(score: number): number {
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

function setAgentListMessage(message: string): void {
  const container = document.getElementById('agentList');
  if (!container) return;
  container.replaceChildren();
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = message;
  container.append(empty);
}

async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      setAgentListMessage('No agents found');
      return;
    }

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
      container.append(item);
    }
  } catch {
    setAgentListMessage('Unable to load agents');
  }
}

async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!el) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const ts = stored['ghost-last-sync'];
  el.textContent = typeof ts === 'number' ? new Date(ts).toLocaleTimeString() : 'never';
}

function updateUI(data: PopupScorePayload): void {
  const score = Number.isFinite(data.score) ? data.score : 0;
  const level = Number.isFinite(data.level) ? Math.max(0, Math.min(4, data.level ?? 0)) : deriveLevel(score);
  const signals = Array.isArray(data.signals) && data.signals.length === EMPTY_SIGNALS.length
    ? data.signals
    : EMPTY_SIGNALS;

  const scoreEl = document.getElementById('scoreValue');
  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(score);
  }

  const levelBadge = document.getElementById('levelBadge');
  if (levelBadge) {
    levelBadge.textContent = `Level ${level}`;
    levelBadge.className = `level-badge level-${level}`;
  }

  const platform = document.getElementById('platform');
  if (platform) {
    platform.textContent = data.platform || 'Browser extension';
  }

  updateSignalList(signals);

  const banner = document.getElementById('alertBanner');
  if (banner) {
    updateAlertBanner(banner as HTMLElement, level);
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response?: { score?: number }) => {
    if (chrome.runtime.lastError) {
      updateConnectionIndicator(false);
      return;
    }

    if (typeof response?.score === 'number') {
      updateUI({ score: response.score, signals: EMPTY_SIGNALS });
    }
  });
}

document.addEventListener('DOMContentLoaded', async () => {
  const signalList = document.getElementById('signalList');
  const sessionDuration = document.getElementById('sessionDuration');
  if (signalList) {
    renderSignalList(signalList as HTMLElement);
  }
  if (sessionDuration) {
    startSessionTimer(sessionDuration as HTMLElement);
  }

  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);
  updateUI({ score: 0, signals: EMPTY_SIGNALS });

  if (auth.authenticated) {
    await loadAgentList();
    requestScore();
  } else {
    setAgentListMessage('Not connected to gateway');
  }

  await loadSyncStatus();
});

window.addEventListener('unload', () => {
  stopSessionTimer();
});
