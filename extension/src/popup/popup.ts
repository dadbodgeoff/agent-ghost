/**
 * Popup script — displays convergence score, session state, and gateway agents.
 */

import { getAuthState } from '../background/auth-sync';
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

let sessionStart = Date.now();

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

function scoreToLevel(score: number): number {
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

function renderSignals(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = '';
  const values = Array.from({ length: SIGNAL_NAMES.length }, (_, index) => signals[index] ?? 0);

  values.forEach((value, index) => {
    const row = document.createElement('div');
    row.className = 'signal-row';

    const name = document.createElement('span');
    name.className = 'signal-name';
    name.textContent = SIGNAL_NAMES[index];

    const score = document.createElement('span');
    score.className = 'signal-value';
    score.textContent = value.toFixed(3);

    const bar = document.createElement('div');
    bar.className = 'signal-bar';

    const fill = document.createElement('div');
    fill.className = 'signal-bar-fill';
    fill.style.width = `${Math.max(0, Math.min(1, value)) * 100}%`;
    fill.style.background = scoreColor(value);

    bar.append(fill);
    row.append(name, score, bar);
    container.append(row);
  });
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
        (agent) =>
          `<div class="agent-list-item">` +
          `<span class="agent-name">${agent.name || agent.id}</span>` +
          `<span class="agent-state">${agent.state}</span>` +
          `</div>`,
      )
      .join('');
  } catch {
    container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
  }
}

async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!el) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const ts = stored['ghost-last-sync'];
  el.textContent = typeof ts === 'number' ? new Date(ts).toLocaleTimeString() : 'never';
}

function startSessionTimer(): void {
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

function updateAlert(level: number): void {
  const banner = document.getElementById('alertBanner');
  if (!banner) return;

  banner.className = 'alert-banner';
  banner.textContent = '';

  if (level >= 3) {
    banner.classList.add('active', 'alert-danger');
    banner.textContent =
      level === 4 ? 'Intervention Level 4: external escalation active.' : 'Intervention Level 3: session may be terminated.';
    return;
  }

  if (level >= 2) {
    banner.classList.add('active', 'alert-warning');
    banner.textContent = 'Intervention Level 2: acknowledgment recommended.';
  }
}

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const platformEl = document.getElementById('platform');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.score);
  }

  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  if (platformEl) {
    platformEl.textContent = 'Browser extension';
  }

  renderSignals(data.signals);
  updateAlert(data.level);
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) return;
    if (!response || typeof response.score !== 'number') return;

    updateUI({
      score: response.score,
      level: scoreToLevel(response.score),
      signals: [response.score, 0, 0, 0, 0, 0, 0],
    });
  });
}

async function initPopup(): Promise<void> {
  renderSignals([0, 0, 0, 0, 0, 0, 0]);

  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.lastValidated > 0) {
    sessionStart = auth.lastValidated;
  }

  startSessionTimer();

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
}

void initPopup();
