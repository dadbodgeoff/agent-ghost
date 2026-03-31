/**
 * Popup script — displays convergence score, signals, connection status, and agents.
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

const LEVEL_LABELS = [
  'Level 0 - Normal',
  'Level 1 - Soft',
  'Level 2 - Active',
  'Level 3 - Hard',
  'Level 4 - External',
];

const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'];

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

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function deriveLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map(
    (name, index) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${index}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${index}" style="width: 0%"></div>
        </div>
      </div>
    `,
  ).join('');
}

function updateAlert(level: number): void {
  const banner = document.getElementById('alertBanner');
  if (!banner) return;

  if (level >= 3) {
    banner.className = 'alert-banner active alert-danger';
    banner.textContent =
      level === 4
        ? 'Intervention Level 4 - External escalation active'
        : 'Intervention Level 3 - Session may be terminated';
    return;
  }

  if (level >= 2) {
    banner.className = 'alert-banner active alert-warning';
    banner.textContent = 'Intervention Level 2 - Acknowledgment required';
    return;
  }

  banner.className = 'alert-banner';
  banner.textContent = '';
}

function updateUI(data: { score: number; level?: number; signals?: number[]; platform?: string }): void {
  const score = Number.isFinite(data.score) ? data.score : 0;
  const level = Number.isFinite(data.level) ? data.level : deriveLevel(score);

  const scoreEl = document.getElementById('scoreValue');
  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(score);
  }

  const badge = document.getElementById('levelBadge');
  if (badge) {
    badge.textContent = LEVEL_LABELS[level] ?? `Level ${level}`;
    badge.className = `level-badge ${LEVEL_CLASSES[level] ?? 'level-0'}`;
  }

  const signals = Array.isArray(data.signals) ? data.signals : new Array(SIGNAL_NAMES.length).fill(0);
  signals.slice(0, SIGNAL_NAMES.length).forEach((value, index) => {
    const numericValue = Number.isFinite(value) ? value : 0;
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`) as HTMLElement | null;
    if (valueEl) valueEl.textContent = numericValue.toFixed(3);
    if (barEl) {
      barEl.style.width = `${Math.max(0, Math.min(100, numericValue * 100)).toFixed(0)}%`;
      barEl.style.background = scoreColor(numericValue);
    }
  });

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = data.platform ?? 'Native monitor';
  }

  updateAlert(level);
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

  timerEl.textContent = '0h 0m 0s';
  window.setInterval(() => {
    const elapsed = Math.floor((Date.now() - sessionStart) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
  }, 1000);
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response?: { score?: number }) => {
    if (chrome.runtime.lastError || typeof response?.score !== 'number') {
      return;
    }

    updateUI({
      score: response.score,
      level: deriveLevel(response.score),
      signals: new Array(SIGNAL_NAMES.length).fill(0),
    });
  });
}

async function initPopup(): Promise<void> {
  renderSignalList();
  startSessionTimer();
  await initAuthSync();

  const auth = getAuthState();
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
  requestScore();
}

document.addEventListener('DOMContentLoaded', () => {
  sessionStart = Date.now();
  void initPopup();
});
