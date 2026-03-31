/**
 * Popup script — renders the convergence status for the current session.
 */

import { getAgents } from '../background/gateway-client';
import { getAuthState, initAuthSync } from '../background/auth-sync';

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

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function computeLevel(score: number): number {
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
    (name, i) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${i}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${i}" style="width:0%"></div>
        </div>
      </div>
    `,
  ).join('');
}

function updateUI(data: { score: number; level?: number; signals?: number[]; platform?: string }): void {
  const score = Number.isFinite(data.score) ? data.score : 0;
  const level = Number.isFinite(data.level) ? data.level : computeLevel(score);
  const signals = Array.isArray(data.signals) ? data.signals : [];

  const scoreEl = document.getElementById('scoreValue');
  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(score);
  }

  const levelBadge = document.getElementById('levelBadge');
  if (levelBadge) {
    levelBadge.textContent = LEVEL_LABELS[level] ?? `Level ${level}`;
    levelBadge.className = `level-badge ${LEVEL_CLASSES[level] ?? 'level-0'}`;
  }

  const platform = document.getElementById('platform');
  if (platform) {
    platform.textContent = data.platform ?? 'Current session';
  }

  SIGNAL_NAMES.forEach((_, i) => {
    const value = typeof signals[i] === 'number' ? signals[i] : 0;
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`) as HTMLElement | null;
    if (valueEl) valueEl.textContent = value.toFixed(3);
    if (barEl) {
      barEl.style.width = `${Math.max(0, Math.min(100, value * 100)).toFixed(0)}%`;
      barEl.style.background = scoreColor(value);
    }
  });

  const banner = document.getElementById('alertBanner');
  if (banner) {
    if (level >= 3) {
      banner.className = 'alert-banner active alert-danger';
      banner.textContent =
        level === 4
          ? 'Intervention Level 4 - External escalation active'
          : `Intervention Level ${level} - Session may be terminated`;
    } else if (level >= 2) {
      banner.className = 'alert-banner active alert-warning';
      banner.textContent = 'Intervention Level 2 - Acknowledgment required';
    } else {
      banner.className = 'alert-banner';
      banner.textContent = '';
    }
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) return;
    if (!response || typeof response.score !== 'number') return;
    updateUI({ score: response.score });
  });
}

function startSessionTimer(): void {
  const el = document.getElementById('sessionDuration');
  if (!el) return;

  const render = () => {
    const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    el.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  render();
  setInterval(render, 1000);
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
        (agent) => `
          <div class="agent-list-item">
            <span class="agent-name">${agent.name || agent.id}</span>
            <span class="agent-state">${agent.state}</span>
          </div>
        `,
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
    el.textContent = typeof ts === 'number' ? new Date(ts).toLocaleTimeString() : 'never';
  } catch {
    el.textContent = 'unknown';
  }
}

async function initialize(): Promise<void> {
  renderSignalList();
  startSessionTimer();
  requestScore();
  setInterval(requestScore, 5000);

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
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    void initialize();
  });
} else {
  void initialize();
}
