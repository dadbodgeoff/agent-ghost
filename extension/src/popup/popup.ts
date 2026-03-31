/**
 * Popup script — displays convergence score and signals.
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

let sessionStart = Date.now();

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

function renderSignalList(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map((name, index) => {
    const value = signals[index] ?? 0;
    const width = Math.max(0, Math.min(value, 1)) * 100;
    return `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value">${value.toFixed(3)}</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" style="width:${width}%; background:${scoreColor(value)}"></div>
        </div>
      </div>
    `;
  }).join('');
}

function updateAlert(level: number): void {
  const banner = document.getElementById('alertBanner');
  if (!banner) return;

  if (level >= 3) {
    banner.className = 'alert-banner active alert-danger';
    banner.textContent =
      level === 4
        ? 'External escalation active for this session.'
        : `Convergence level ${level} detected. Session may need intervention.`;
    return;
  }

  if (level >= 2) {
    banner.className = 'alert-banner active alert-warning';
    banner.textContent = 'Convergence is rising. Consider slowing the session down.';
    return;
  }

  banner.className = 'alert-banner';
  banner.textContent = '';
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
        (a) =>
          `<div class="agent-list-item">` +
          `<span class="agent-name">${a.name || a.id}</span>` +
          `<span class="agent-state">${a.state}</span>` +
          `</div>`
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
  if (ts && typeof ts === 'number') {
    el.textContent = new Date(ts).toLocaleTimeString();
  } else {
    el.textContent = 'never';
  }
}

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] ?? `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignalList(data.signals);
  updateAlert(data.level);
}

function updateSessionDuration(): void {
  const durationEl = document.getElementById('sessionDuration');
  if (!durationEl) return;

  const elapsedSeconds = Math.floor((Date.now() - sessionStart) / 1000);
  const minutes = Math.floor(elapsedSeconds / 60);
  const seconds = elapsedSeconds % 60;
  durationEl.textContent = `${minutes}m ${seconds}s`;
}

function updatePlatform(platform: string): void {
  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = platform;
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (!response || response.score === undefined) {
      return;
    }

    const level = response.score > 0.85 ? 4 :
      response.score > 0.7 ? 3 :
      response.score > 0.5 ? 2 :
      response.score > 0.3 ? 1 : 0;

    updateUI({
      score: response.score,
      level,
      signals: [response.score, 0, 0, 0, 0, 0, 0],
    });
  });
}

function scoreColor(score: number): string {
  if (score > 0.85) return '#ef4444';
  if (score > 0.7) return '#f97316';
  if (score > 0.5) return '#f59e0b';
  if (score > 0.3) return '#eab308';
  return '#22c55e';
}

document.addEventListener('DOMContentLoaded', async () => {
  renderSignalList([0, 0, 0, 0, 0, 0, 0]);
  updateSessionDuration();
  updatePlatform('Browser extension');

  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  if (auth.gatewayUrl) {
    updatePlatform(auth.gatewayUrl);
  }

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
  setInterval(updateSessionDuration, 1000);
});
