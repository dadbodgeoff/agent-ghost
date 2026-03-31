import { getAgents, getHealth } from '../background/gateway-client';
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

let sessionStartTime = Date.now();

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

function levelFromScore(score: number): number {
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

function updateScoreDisplay(score: number): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const banner = document.getElementById('alertBanner');
  const level = levelFromScore(score);

  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
    scoreEl.style.color = scoreColor(score);
  }

  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[level] ?? `Level ${level}`;
    levelEl.className = `level-badge level-${Math.min(level, 4)}`;
  }

  if (banner) {
    if (level >= 3) {
      banner.className = 'alert-banner active alert-danger';
      banner.textContent = `Intervention Level ${level} - ${level === 4 ? 'External escalation active' : 'Session may be terminated'}`;
    } else if (level >= 2) {
      banner.className = 'alert-banner active alert-warning';
      banner.textContent = 'Intervention Level 2 - Acknowledgment required';
    } else {
      banner.className = 'alert-banner';
      banner.textContent = '';
    }
  }
}

function updateSignals(signals: number[]): void {
  signals.forEach((value, index) => {
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) {
      valueEl.textContent = value.toFixed(3);
    }
    if (barEl) {
      const safeValue = Math.max(0, Math.min(value, 1));
      (barEl as HTMLElement).style.width = `${(safeValue * 100).toFixed(0)}%`;
      (barEl as HTMLElement).style.background = scoreColor(safeValue);
    }
  });
}

async function requestLatestScore(): Promise<void> {
  await new Promise<void>((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response?: { score?: number }) => {
      if (chrome.runtime.lastError) {
        resolve();
        return;
      }

      const score = typeof response?.score === 'number' ? response.score : 0;
      updateScoreDisplay(score);
      updateSignals(new Array(SIGNAL_NAMES.length).fill(score));
      resolve();
    });
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
          '</div>',
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

function startSessionTimer(): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const tick = () => {
    const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  tick();
  window.setInterval(tick, 1000);
}

async function loadPlatformStatus(): Promise<void> {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;

  const auth = getAuthState();
  if (!auth.authenticated) {
    platformEl.textContent = 'Gateway unavailable';
    return;
  }

  try {
    const health = await getHealth();
    platformEl.textContent = health.version ? `Gateway ${health.version}` : health.status;
  } catch {
    platformEl.textContent = 'Gateway unavailable';
  }
}

async function bootstrap(): Promise<void> {
  renderSignalList();
  startSessionTimer();
  await initAuthSync();

  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await Promise.all([loadAgentList(), loadPlatformStatus()]);
  } else {
    const container = document.getElementById('agentList');
    const platformEl = document.getElementById('platform');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
    if (platformEl) {
      platformEl.textContent = 'Not authenticated';
    }
  }

  await Promise.all([loadSyncStatus(), requestLatestScore()]);
}

document.addEventListener('DOMContentLoaded', () => {
  void bootstrap();
});
