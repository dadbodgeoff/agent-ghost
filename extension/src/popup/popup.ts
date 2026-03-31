/**
 * Popup script — displays convergence score, gateway status, and agent state.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_LABELS = [
  'Role drift',
  'Reply velocity',
  'Thread depth',
  'Escalation risk',
  'Context churn',
  'Tool pressure',
  'Session instability',
];

interface PopupScoreData {
  score: number;
  level: number;
  signals: number[];
}

function updateConnectionIndicator(connected: boolean): void {
  const dot = document.getElementById('statusDot');
  const label = document.getElementById('statusLabel');
  if (dot) {
    dot.classList.remove('connected', 'disconnected');
    dot.classList.add(connected ? 'connected' : 'disconnected');
  }
  if (label) {
    label.classList.remove('connected', 'disconnected');
    label.classList.add(connected ? 'connected' : 'disconnected');
    label.textContent = connected ? 'Connected' : 'Disconnected';
  }
}

function renderSignals(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  const normalized = SIGNAL_LABELS.map((label, index) => ({
    label,
    value: signals[index] ?? 0,
  }));

  container.innerHTML = normalized
    .map(
      ({ label, value }) =>
        `<div class="signal-row">` +
        `<span class="signal-name">${label}</span>` +
        `<span style="display:flex;align-items:center;">` +
        `<span class="signal-value">${value.toFixed(2)}</span>` +
        `<span class="signal-bar"><span class="signal-bar-fill" style="width:${Math.max(
          0,
          Math.min(value, 1),
        ) * 100}%;background:${value >= 0.7 ? '#ef4444' : value >= 0.4 ? '#f59e0b' : '#22c55e'};"></span></span>` +
        `</span>` +
        `</div>`,
    )
    .join('');
}

function updateAlert(level: number): void {
  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (level >= 3) {
    alertEl.className = `alert-banner active ${level >= 4 ? 'alert-danger' : 'alert-warning'}`;
    alertEl.textContent =
      level >= 4
        ? 'Critical convergence detected. Pause or switch tasks now.'
        : `Convergence level ${level} detected. Consider taking a break.`;
    return;
  }

  alertEl.className = 'alert-banner';
  alertEl.textContent = '';
}

function updateUI(data: PopupScoreData): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignals(data.signals);
  updateAlert(data.level);
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
  el.textContent = ts && typeof ts === 'number' ? new Date(ts).toLocaleTimeString() : 'never';
}

function updateSessionDuration(sessionStart: number): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const elapsedMinutes = Math.floor((Date.now() - sessionStart) / 60000);
  timerEl.textContent = `${elapsedMinutes}m`;
}

function updatePlatformLabel(): void {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;

  const manifest = chrome.runtime.getManifest();
  platformEl.textContent = manifest.manifest_version === 3 ? 'Browser extension' : 'Browser add-on';
}

function requestScore(): Promise<PopupScoreData> {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
      const score = typeof response?.score === 'number' ? response.score : 0;
      const level =
        score > 0.85 ? 4 : score > 0.7 ? 3 : score > 0.5 ? 2 : score > 0.3 ? 1 : 0;
      resolve({
        score,
        level,
        signals: Array.from({ length: SIGNAL_LABELS.length }, () => 0),
      });
    });
  });
}

async function bootstrap(): Promise<void> {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  updatePlatformLabel();

  const sessionStart = Date.now();
  updateSessionDuration(sessionStart);
  window.setInterval(() => updateSessionDuration(sessionStart), 60_000);

  const score = await requestScore();
  updateUI(score);

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

void bootstrap().catch(() => {
  updateConnectionIndicator(false);
});
