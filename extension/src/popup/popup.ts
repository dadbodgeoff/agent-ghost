/**
 * Popup script — displays convergence score and signals.
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
] as const;

const LEVEL_LABELS = [
  'Level 0 - Normal',
  'Level 1 - Soft',
  'Level 2 - Active',
  'Level 3 - Hard',
  'Level 4 - External',
] as const;

const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'] as const;

type PopupScoreData = {
  score: number;
  level: number;
  signals: number[];
  platform?: string;
};

let sessionStart = Date.now();

function clampLevel(level: number): number {
  return Math.max(0, Math.min(LEVEL_LABELS.length - 1, level));
}

function inferLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

function normalizeScorePayload(payload: unknown): PopupScoreData | null {
  if (typeof payload === 'number' && Number.isFinite(payload)) {
    return {
      score: payload,
      level: inferLevel(payload),
      signals: Array.from({ length: SIGNAL_NAMES.length }, () => 0),
    };
  }

  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const scorePayload = payload as {
    composite_score?: unknown;
    score?: unknown;
    level?: unknown;
    signals?: unknown;
    platform?: unknown;
  };
  const rawScore =
    typeof scorePayload.composite_score === 'number'
      ? scorePayload.composite_score
      : typeof scorePayload.score === 'number'
        ? scorePayload.score
        : 0;
  const rawLevel =
    typeof scorePayload.level === 'number' ? scorePayload.level : inferLevel(rawScore);
  const signals = Array.isArray(scorePayload.signals)
    ? scorePayload.signals
        .map((value) => (typeof value === 'number' && Number.isFinite(value) ? value : 0))
        .slice(0, SIGNAL_NAMES.length)
    : [];

  while (signals.length < SIGNAL_NAMES.length) {
    signals.push(0);
  }

  return {
    score: rawScore,
    level: rawLevel,
    signals,
    platform: typeof scorePayload.platform === 'string' ? scorePayload.platform : undefined,
  };
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

/**
 * Update the connection indicator (statusDot + statusLabel).
 */
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

/**
 * Fetch and render the agent list from the gateway.
 */
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

async function requestScore(): Promise<void> {
  await new Promise<void>((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
      if (chrome.runtime.lastError) {
        resolve();
        return;
      }

      const normalized = normalizeScorePayload(response?.score ?? response?.data ?? response);
      if (normalized) {
        updateUI(normalized);
      }
      resolve();
    });
  });
}

/**
 * Load and display the last sync time from storage.
 */
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

function updateSessionDuration(): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const elapsed = Math.floor((Date.now() - sessionStart) / 1000);
  const hours = Math.floor(elapsed / 3600);
  const minutes = Math.floor((elapsed % 3600) / 60);
  const seconds = elapsed % 60;
  timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
}

function startSessionTimer(): void {
  sessionStart = Date.now();
  updateSessionDuration();
  window.setInterval(updateSessionDuration, 1000);
}

function updateUI(data: PopupScoreData): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const level = clampLevel(data.level);

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.score);
  }
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[level];
    levelEl.className = `level-badge ${LEVEL_CLASSES[level]}`;
  }

  data.signals.forEach((val, i) => {
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`);
    if (valueEl) {
      valueEl.textContent = val.toFixed(3);
    }
    if (barEl) {
      (barEl as HTMLElement).style.width = `${Math.max(0, Math.min(1, val)) * 100}%`;
      (barEl as HTMLElement).style.background = scoreColor(val);
    }
  });

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = data.platform ?? 'Active browser session';
  }

  const alertEl = document.getElementById('alertBanner');
  if (data.level >= 3 && alertEl) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      data.level === 4
        ? 'Intervention Level 4 - External escalation active'
        : `Convergence level ${data.level} detected. Consider taking a break.`;
  } else if (data.level >= 2 && alertEl) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2 - Acknowledgment recommended';
  } else if (alertEl) {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

document.addEventListener('DOMContentLoaded', () => {
  renderSignalList();
  startSessionTimer();

  chrome.runtime.onMessage.addListener((message) => {
    if (message?.type !== 'score_update') return;
    const normalized = normalizeScorePayload(message.data);
    if (normalized) {
      updateUI(normalized);
    }
  });

  void (async () => {
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

    await Promise.all([loadSyncStatus(), requestScore()]);
  })();
});
