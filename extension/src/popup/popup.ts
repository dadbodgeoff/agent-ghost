/**
 * Popup script — displays convergence score and signals.
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

type PopupScoreState = {
  compositeScore: number;
  level: number;
  signals: number[];
  platform?: string;
  lastSync?: number;
};

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function createEmptyState(text: string): HTMLSpanElement {
  const empty = document.createElement('span');
  empty.className = 'agent-list-empty';
  empty.textContent = text;
  return empty;
}

function renderSignalList(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.replaceChildren(
    ...SIGNAL_NAMES.map((name, index) => {
      const value = signals[index] ?? 0;

      const row = document.createElement('div');
      row.className = 'signal-row';

      const label = document.createElement('span');
      label.className = 'signal-name';
      label.textContent = name;

      const signalValue = document.createElement('span');
      signalValue.className = 'signal-value';
      signalValue.textContent = value.toFixed(3);

      const bar = document.createElement('div');
      bar.className = 'signal-bar';

      const barFill = document.createElement('div');
      barFill.className = 'signal-bar-fill';
      barFill.style.width = `${Math.max(0, Math.min(value, 1)) * 100}%`;
      barFill.style.background = scoreColor(value);

      bar.append(barFill);
      row.append(label, signalValue, bar);
      return row;
    }),
  );
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

async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      container.replaceChildren(createEmptyState('No agents found'));
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
    container.replaceChildren(createEmptyState('Unable to load agents'));
  }
}

async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!el) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const ts = stored['ghost-last-sync'];
  el.textContent = typeof ts === 'number' && ts > 0 ? new Date(ts).toLocaleTimeString() : 'never';
}

function updateUI(data: PopupScoreState): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const platformEl = document.getElementById('platform');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) {
    scoreEl.textContent = data.compositeScore.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.compositeScore);
  }

  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignalList(data.signals);

  if (platformEl) {
    platformEl.textContent = data.platform || 'Unknown';
  }

  if (typeof data.lastSync === 'number' && data.lastSync > 0) {
    const syncStatus = document.getElementById('syncStatus');
    if (syncStatus) {
      syncStatus.textContent = new Date(data.lastSync).toLocaleTimeString();
    }
  }

  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = 'alert-banner active alert-danger';
      alertEl.textContent = `Intervention Level ${data.level} detected. Consider taking a break.`;
    } else if (data.level >= 2) {
      alertEl.className = 'alert-banner active alert-warning';
      alertEl.textContent = 'Intervention Level 2 detected. Stay aware of conversation drift.';
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response) return;
    updateUI(response as PopupScoreState);
  });
}

const sessionStart = Date.now();

function updateSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

renderSignalList(Array(7).fill(0));
updateSessionTimer();
requestScore();
setInterval(updateSessionTimer, 60_000);
setInterval(requestScore, 5_000);

(async () => {
  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.replaceChildren(createEmptyState('Not connected to gateway'));
    }
  }

  await loadSyncStatus();
})();
