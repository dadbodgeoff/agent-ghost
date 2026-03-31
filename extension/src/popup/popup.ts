/**
 * Popup script — displays convergence score, signals, and gateway state.
 */

import { getAuthState } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_LABELS = [
  'Self-reference',
  'Urgency',
  'Dependency',
  'Boundary drift',
  'Flattery',
  'Persistence',
  'Escalation',
];

let sessionTimer: ReturnType<typeof setInterval> | null = null;

function inferLevel(score: number): number {
  return score > 0.85 ? 4 : score > 0.7 ? 3 : score > 0.5 ? 2 : score > 0.3 ? 1 : 0;
}

function updateConnectionIndicator(connected: boolean): void {
  const dot = document.getElementById('statusDot');
  const label = document.getElementById('statusLabel');
  if (dot) {
    dot.classList.remove('connected', 'disconnected');
    dot.classList.add(connected ? 'connected' : 'disconnected');
    dot.setAttribute(
      'aria-label',
      connected ? 'Connection status: connected' : 'Connection status: disconnected',
    );
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

  try {
    const stored = await chrome.storage.local.get('ghost-last-sync');
    const ts = stored['ghost-last-sync'];
    el.textContent = ts && typeof ts === 'number' ? new Date(ts).toLocaleTimeString() : 'never';
  } catch {
    el.textContent = 'unknown';
  }
}

async function loadPlatform(): Promise<void> {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;

  try {
    const stored = await chrome.storage.local.get('ghost-last-platform');
    platformEl.textContent =
      typeof stored['ghost-last-platform'] === 'string' ? stored['ghost-last-platform'] : 'Unknown';
  } catch {
    platformEl.textContent = 'Unknown';
  }
}

function renderSignals(signals: number[]): void {
  const signalList = document.getElementById('signalList');
  if (!signalList) return;

  signalList.innerHTML = signals
    .slice(0, SIGNAL_LABELS.length)
    .map((value, index) => {
      const pct = Math.max(0, Math.min(100, Math.round(value * 100)));
      const color = value >= 0.7 ? '#ef4444' : value >= 0.4 ? '#f59e0b' : '#22c55e';
      return (
        `<div class="signal-row">` +
        `<span class="signal-name">${SIGNAL_LABELS[index]}</span>` +
        `<span style="display:flex;align-items:center;gap:8px;">` +
        `<span class="signal-value">${value.toFixed(2)}</span>` +
        `<span class="signal-bar"><span class="signal-bar-fill" style="width:${pct}%;background:${color};"></span></span>` +
        `</span>` +
        `</div>`
      );
    })
    .join('');
}

function updateAlert(level: number): void {
  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (level >= 3) {
    alertEl.className = `alert-banner active ${level >= 4 ? 'alert-danger' : 'alert-warning'}`;
    alertEl.textContent = `Convergence level ${level} detected. Consider taking a break.`;
    return;
  }

  alertEl.className = 'alert-banner';
  alertEl.textContent = '';
}

function updateSessionDuration(sessionStart: number): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  timerEl.textContent = elapsed <= 0 ? '<1m' : `${elapsed}m`;
}

function updateUI(data: { score: number; signals: number[] }): void {
  const level = inferLevel(data.score);
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
  }
  if (levelEl) {
    levelEl.textContent = `Level ${level}`;
    levelEl.className = `level-badge level-${level}`;
  }

  renderSignals(data.signals);
  updateAlert(level);
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) {
      return;
    }
    if (!response || typeof response.score !== 'number') {
      return;
    }

    const signals = Array.isArray(response.signals)
      ? response.signals.map((value: unknown) => (typeof value === 'number' ? value : 0))
      : [0, 0, 0, 0, 0, 0, 0];

    updateUI({ score: response.score, signals });
  });
}

async function init(): Promise<void> {
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  const sessionStart = Date.now();
  updateSessionDuration(sessionStart);
  sessionTimer = setInterval(() => {
    updateSessionDuration(sessionStart);
  }, 60000);

  requestScore();
  await Promise.all([loadPlatform(), loadSyncStatus()]);

  if (auth.authenticated) {
    await loadAgentList();
    return;
  }

  const container = document.getElementById('agentList');
  if (container) {
    container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
  }
}

window.addEventListener('unload', () => {
  if (sessionTimer) {
    clearInterval(sessionTimer);
    sessionTimer = null;
  }
});

void init();
