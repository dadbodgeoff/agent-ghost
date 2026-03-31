import { initAuthSync } from '../background/auth-sync.js';
import { getAgents } from '../background/gateway-client.js';

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
const DEFAULT_SIGNALS = Array<number>(SIGNAL_NAMES.length).fill(0);

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
  window.setInterval(update, 1000);
}

function updateDisplay(payload: unknown): void {
  const data = (payload ?? {}) as {
    score?: number;
    composite_score?: number;
    level?: number;
    signals?: number[];
    platform?: string;
  };

  const score = typeof data.composite_score === 'number'
    ? data.composite_score
    : typeof data.score === 'number'
      ? data.score
      : 0;
  const level = typeof data.level === 'number' ? data.level : 0;
  const signals = Array.isArray(data.signals) ? data.signals : DEFAULT_SIGNALS;

  const scoreEl = document.getElementById('scoreValue');
  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(score);
  }

  const badge = document.getElementById('levelBadge');
  if (badge) {
    badge.textContent = LEVEL_LABELS[level] || `Level ${level}`;
    badge.className = `level-badge ${LEVEL_CLASSES[level] || 'level-0'}`;
  }

  signals.slice(0, SIGNAL_NAMES.length).forEach((value, index) => {
    const numeric = Number.isFinite(value) ? value : 0;
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) valueEl.textContent = numeric.toFixed(3);
    if (barEl) {
      (barEl as HTMLElement).style.width = `${Math.max(0, Math.min(1, numeric)) * 100}%`;
      (barEl as HTMLElement).style.background = scoreColor(numeric);
    }
  });

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = data.platform || 'Unknown';
  }

  const banner = document.getElementById('alertBanner');
  if (banner) {
    if (level >= 3) {
      banner.className = 'alert-banner active alert-danger';
      banner.textContent =
        `Intervention Level ${level} - ${level === 4 ? 'External escalation active' : 'Session may be terminated'}`;
    } else if (level >= 2) {
      banner.className = 'alert-banner active alert-warning';
      banner.textContent = 'Intervention Level 2 - Acknowledgment required';
    } else {
      banner.className = 'alert-banner';
      banner.textContent = '';
    }
  }
}

async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  container.innerHTML = '<span class="agent-list-empty">Loading agents...</span>';

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

function requestStatus(): void {
  chrome.runtime.sendMessage({ type: 'get_status' }, (response) => {
    if (chrome.runtime.lastError || !response) {
      return;
    }

    updateConnectionIndicator(Boolean(response.connected));
    if (response.latestScore) {
      updateDisplay(response.latestScore);
    }
  });
}

async function hydrateGatewayState(): Promise<void> {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Connect the dashboard to a gateway to load agents</span>';
    }
  }

  const platformEl = document.getElementById('platform');
  if (platformEl && !platformEl.textContent?.trim()) {
    platformEl.textContent = auth.authenticated ? 'Gateway' : 'Not connected';
  }

  await loadSyncStatus();
}

document.addEventListener('DOMContentLoaded', () => {
  renderSignalList();
  startSessionTimer();
  requestStatus();
  void hydrateGatewayState();

  chrome.runtime.onMessage.addListener((message) => {
    if (message.type === 'score_update') {
      updateDisplay(message.data);
    }
  });

  window.setInterval(requestStatus, 5000);
});
