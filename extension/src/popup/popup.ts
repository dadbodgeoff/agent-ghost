/**
 * Popup script — renders convergence status, signal breakdown, and gateway state.
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

const LEVEL_LABELS = [
  'Level 0 - Normal',
  'Level 1 - Soft',
  'Level 2 - Active',
  'Level 3 - Hard',
  'Level 4 - External',
];

const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'];

type ScorePayload = {
  composite_score?: number;
  score?: number;
  level?: number;
  signals?: number[];
  platform?: string;
};

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

function updateDisplay(data: ScorePayload | null | undefined): void {
  const scoreValue = document.getElementById('scoreValue');
  const levelBadge = document.getElementById('levelBadge');
  const platform = document.getElementById('platform');

  const score =
    typeof data?.composite_score === 'number'
      ? data.composite_score
      : typeof data?.score === 'number'
        ? data.score
        : 0;

  const inferredLevel =
    score > 0.85 ? 4 :
    score > 0.7 ? 3 :
    score > 0.5 ? 2 :
    score > 0.3 ? 1 : 0;
  const level = typeof data?.level === 'number' ? data.level : inferredLevel;
  const clampedLevel = Math.max(0, Math.min(level, LEVEL_LABELS.length - 1));

  if (scoreValue) {
    scoreValue.textContent = score.toFixed(2);
    (scoreValue as HTMLElement).style.color = scoreColor(score);
  }

  if (levelBadge) {
    levelBadge.textContent = LEVEL_LABELS[clampedLevel] ?? `Level ${clampedLevel}`;
    levelBadge.className = `level-badge ${LEVEL_CLASSES[clampedLevel] ?? 'level-0'}`;
  }

  if (platform && data?.platform) {
    platform.textContent = data.platform;
  }

  const signals = Array.isArray(data?.signals) ? data.signals : [];
  for (let index = 0; index < SIGNAL_NAMES.length; index += 1) {
    const rawValue = signals[index];
    const value = typeof rawValue === 'number' && Number.isFinite(rawValue) ? rawValue : 0;
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) {
      valueEl.textContent = value.toFixed(3);
    }
    if (barEl) {
      (barEl as HTMLElement).style.width = `${Math.max(0, Math.min(value, 1)) * 100}%`;
      (barEl as HTMLElement).style.background = scoreColor(value);
    }
  }

  updateAlert(clampedLevel);
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
  window.setInterval(render, 1000);
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
  const timestamp = stored['ghost-last-sync'];
  el.textContent = typeof timestamp === 'number' ? new Date(timestamp).toLocaleTimeString() : 'never';
}

function requestStatus(): void {
  chrome.runtime.sendMessage({ type: 'get_status' }, (response: {
    connected?: boolean;
    latestScore?: ScorePayload | null;
  } | undefined) => {
    if (chrome.runtime.lastError || !response) return;
    updateConnectionIndicator(Boolean(response.connected));
    if (response.latestScore) {
      updateDisplay(response.latestScore);
    }
  });
}

document.addEventListener('DOMContentLoaded', async () => {
  renderSignalList();
  startSessionTimer();
  requestStatus();

  chrome.runtime.onMessage.addListener((message: { type?: string; data?: ScorePayload }) => {
    if (message.type === 'score_update') {
      updateDisplay(message.data);
    }
  });

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
  window.setInterval(requestStatus, 5000);
});
