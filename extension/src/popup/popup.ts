/**
 * Popup script — renders extension status, score, signals, and agent list.
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

const sessionStart = Date.now();

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

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map(
    (name, index) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${index}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${index}" style="width:0%"></div>
        </div>
      </div>
    `,
  ).join('');
}

function updateSessionTimer(): void {
  const el = document.getElementById('sessionDuration');
  if (!el) return;

  const elapsed = Math.floor((Date.now() - sessionStart) / 1000);
  const hours = Math.floor(elapsed / 3600);
  const minutes = Math.floor((elapsed % 3600) / 60);
  const seconds = elapsed % 60;
  el.textContent = `${hours}h ${minutes}m ${seconds}s`;
}

function updatePlatform(platform?: string): void {
  const el = document.getElementById('platform');
  if (el) {
    el.textContent = platform || 'Gateway';
  }
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
  const scoreEl = document.getElementById('scoreValue');
  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.score);
  }

  const level = typeof data.level === 'number' ? data.level : scoreToLevel(data.score);
  const badge = document.getElementById('levelBadge');
  if (badge) {
    badge.textContent = LEVEL_LABELS[level] || `Level ${level}`;
    badge.className = `level-badge ${LEVEL_CLASSES[level] || 'level-0'}`;
  }

  data.signals?.forEach((value, index) => {
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) {
      valueEl.textContent = value.toFixed(3);
    }
    if (barEl) {
      (barEl as HTMLElement).style.width = `${Math.max(0, Math.min(100, value * 100))}%`;
      (barEl as HTMLElement).style.background = scoreColor(value);
    }
  });

  updatePlatform(data.platform);
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

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response?: { score?: number }) => {
    if (chrome.runtime.lastError || response?.score === undefined) {
      return;
    }

    updateUI({
      score: response.score,
      level: scoreToLevel(response.score),
      signals: new Array(SIGNAL_NAMES.length).fill(0),
    });
  });
}

document.addEventListener('DOMContentLoaded', async () => {
  renderSignalList();
  updateSessionTimer();
  setInterval(updateSessionTimer, 1000);

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

  requestScore();
  await loadSyncStatus();
});
