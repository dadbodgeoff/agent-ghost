/**
 * Popup script — displays convergence score, signals, auth state, and agent status.
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

let sessionStartTime = Date.now();

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

function updateConnectionIndicator(connected: boolean): void {
  const dot = document.getElementById('statusDot');
  const label = document.getElementById('statusLabel');

  if (dot) {
    dot.className = `status-dot ${connected ? 'connected' : 'disconnected'}`;
    dot.setAttribute('aria-label', connected ? 'Connected' : 'Disconnected');
  }

  if (label) {
    label.className = `status-label ${connected ? 'connected' : 'disconnected'}`;
    label.textContent = connected ? 'Connected' : 'Disconnected';
  }
}

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map(
    (name, i) => `
      <div class="signal-row" role="listitem">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${i}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${i}" style="width: 0%"></div>
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
        : `Intervention Level ${level} - Session may be terminated`;
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

function updateSignals(signals: number[]): void {
  signals.forEach((value, i) => {
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`);

    if (valueEl) valueEl.textContent = value.toFixed(3);
    if (barEl) {
      barEl.style.width = `${Math.max(0, Math.min(1, value)) * 100}%`;
      barEl.style.background = scoreColor(value);
    }
  });
}

function updateUI(data: { score: number; level: number; signals: number[]; platform?: string }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const platformEl = document.getElementById('platform');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    scoreEl.setAttribute('style', `color: ${scoreColor(data.score)}`);
  }

  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] ?? `Level ${data.level}`;
    levelEl.className = `level-badge ${LEVEL_CLASSES[data.level] ?? 'level-0'}`;
  }

  if (platformEl) {
    platformEl.textContent = data.platform ?? 'Gateway';
  }

  updateSignals(data.signals);
  updateAlert(data.level);
}

async function requestScore(): Promise<void> {
  const response = await new Promise<{ score?: number } | undefined>((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (result) => {
      if (chrome.runtime.lastError) {
        resolve(undefined);
        return;
      }
      resolve(result as { score?: number } | undefined);
    });
  });

  const score = typeof response?.score === 'number' ? response.score : 0;
  updateUI({
    score,
    level: deriveLevel(score),
    signals: Array.from({ length: SIGNAL_NAMES.length }, () => 0),
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
  const syncEl = document.getElementById('syncStatus');
  if (!syncEl) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const timestamp = stored['ghost-last-sync'];
  syncEl.textContent =
    typeof timestamp === 'number' ? new Date(timestamp).toLocaleTimeString() : 'never';
}

function startSessionTimer(): void {
  const el = document.getElementById('sessionDuration');
  if (!el) return;

  const update = () => {
    const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    el.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  update();
  setInterval(update, 1000);
}

async function bootPopup(): Promise<void> {
  renderSignalList();
  startSessionTimer();

  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
  }

  await Promise.all([requestScore(), loadSyncStatus()]);
}

document.addEventListener('DOMContentLoaded', () => {
  void bootPopup();
  setInterval(() => {
    void requestScore();
  }, 5000);
});
