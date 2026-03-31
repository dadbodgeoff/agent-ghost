/**
 * Popup script for the convergence monitor extension.
 *
 * Matches the current popup DOM, hydrates auth from extension storage,
 * and renders a stable fallback UI even when the gateway is unavailable.
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
const REFRESH_INTERVAL_MS = 5_000;

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

function updateAlert(level: number): void {
  const banner = document.getElementById('alertBanner');
  if (!banner) return;

  if (level >= 3) {
    banner.className = 'alert-banner active alert-danger';
    banner.textContent =
      level === 4
        ? 'Intervention Level 4 - External escalation active'
        : 'Intervention Level 3 - Session may require intervention';
    return;
  }

  if (level >= 2) {
    banner.className = 'alert-banner active alert-warning';
    banner.textContent = 'Intervention Level 2 - Acknowledgment recommended';
    return;
  }

  banner.className = 'alert-banner';
  banner.textContent = '';
}

function updateScore(score: number, signals: number[] = []): void {
  const safeScore = Number.isFinite(score) ? score : 0;
  const level = computeLevel(safeScore);

  const scoreEl = document.getElementById('scoreValue');
  if (scoreEl) {
    scoreEl.textContent = safeScore.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(safeScore);
  }

  const levelEl = document.getElementById('levelBadge');
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[level] ?? `Level ${level}`;
    levelEl.className = `level-badge ${LEVEL_CLASSES[level] ?? 'level-0'}`;
  }

  signals.slice(0, SIGNAL_NAMES.length).forEach((value, index) => {
    const safeValue = Number.isFinite(value) ? value : 0;
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) valueEl.textContent = safeValue.toFixed(3);
    if (barEl) {
      (barEl as HTMLElement).style.width = `${Math.max(0, Math.min(100, safeValue * 100)).toFixed(0)}%`;
      (barEl as HTMLElement).style.background = scoreColor(safeValue);
    }
  });

  updateAlert(level);
}

async function requestScore(): Promise<void> {
  await new Promise<void>((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response?: { score?: number }) => {
      if (chrome.runtime.lastError) {
        resolve();
        return;
      }

      if (typeof response?.score === 'number') {
        updateScore(response.score, []);
      }

      resolve();
    });
  });
}

async function loadAgentList(connected: boolean): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  if (!connected) {
    container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    return;
  }

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
  const timestamp = stored['ghost-last-sync'];
  el.textContent =
    typeof timestamp === 'number' ? new Date(timestamp).toLocaleTimeString() : 'never';
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
  setInterval(render, 1_000);
}

async function refreshPopup(): Promise<void> {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  await Promise.all([loadAgentList(auth.authenticated), loadSyncStatus(), requestScore()]);
}

document.addEventListener('DOMContentLoaded', () => {
  renderSignalList();
  startSessionTimer();
  void refreshPopup();
  setInterval(() => {
    void refreshPopup();
  }, REFRESH_INTERVAL_MS);
});
