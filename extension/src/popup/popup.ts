/**
 * Popup script — renders convergence score, signals, agent status, and session metadata.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents, getScores } from '../background/gateway-client';

const LAST_SYNC_KEY = 'ghost-last-sync';
const LAST_PLATFORM_KEY = 'ghost-last-platform';
const SESSION_START_KEY = 'ghost-session-start';

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

type PopupStateResponse = {
  score?: number;
  platform?: string;
  sessionStart?: number;
};

type PopupScoreState = {
  score: number;
  level: number;
  signals: number[];
};

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

function normalizeSignals(signals: number[]): number[] {
  const normalized = signals.slice(0, SIGNAL_NAMES.length);
  while (normalized.length < SIGNAL_NAMES.length) {
    normalized.push(0);
  }
  return normalized;
}

function formatDuration(startedAt: number | null): string {
  if (!startedAt) return 'No active session';

  const elapsed = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
  const hours = Math.floor(elapsed / 3600);
  const minutes = Math.floor((elapsed % 3600) / 60);
  const seconds = elapsed % 60;

  return `${hours}h ${minutes}m ${seconds}s`;
}

function formatPlatformLabel(platform: string | null): string {
  if (!platform) return 'No active session';

  try {
    const url = new URL(platform);
    return url.hostname.replace(/^www\./, '');
  } catch {
    return platform;
  }
}

function renderSignalList(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map((name, index) => {
    const value = signals[index] ?? 0;
    const width = `${Math.max(0, Math.min(100, value * 100)).toFixed(0)}%`;
    return `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value">${value.toFixed(3)}</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" style="width:${width};background:${scoreColor(value)}"></div>
        </div>
      </div>
    `;
  }).join('');
}

function renderScoreState(state: PopupScoreState): void {
  const scoreEl = document.getElementById('scoreValue');
  const badgeEl = document.getElementById('levelBadge');
  const bannerEl = document.getElementById('alertBanner');

  if (scoreEl) {
    scoreEl.textContent = state.score.toFixed(2);
    scoreEl.style.color = scoreColor(state.score);
  }

  if (badgeEl) {
    badgeEl.textContent = LEVEL_LABELS[state.level] ?? `Level ${state.level}`;
    badgeEl.className = `level-badge level-${state.level}`;
  }

  renderSignalList(normalizeSignals(state.signals));

  if (!bannerEl) return;
  if (state.level >= 3) {
    bannerEl.className = 'alert-banner active alert-danger';
    bannerEl.textContent = `Intervention Level ${state.level} - Session may be terminated`;
    return;
  }
  if (state.level >= 2) {
    bannerEl.className = 'alert-banner active alert-warning';
    bannerEl.textContent = 'Intervention Level 2 - Acknowledgment required';
    return;
  }
  bannerEl.className = 'alert-banner';
  bannerEl.textContent = '';
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

function requestPopupState(): Promise<PopupStateResponse> {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_POPUP_STATE' }, (response: PopupStateResponse) => {
      if (chrome.runtime.lastError) {
        resolve({});
        return;
      }
      resolve(response ?? {});
    });
  });
}

async function loadAgentList(isAuthenticated: boolean): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  if (!isAuthenticated) {
    container.innerHTML = '<span class="agent-list-empty">Connect to the gateway to view agents</span>';
    return;
  }

  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      container.innerHTML = '<span class="agent-list-empty">No agents found</span>';
      return;
    }

    container.innerHTML = agents.map((agent) => `
      <div class="agent-list-item">
        <span class="agent-name">${agent.name || agent.id}</span>
        <span class="agent-state">${agent.state}</span>
      </div>
    `).join('');
  } catch {
    container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
  }
}

async function loadSyncStatus(): Promise<void> {
  const syncStatus = document.getElementById('syncStatus');
  if (!syncStatus) return;

  const stored = await chrome.storage.local.get(LAST_SYNC_KEY);
  const lastSync = stored[LAST_SYNC_KEY];
  syncStatus.textContent =
    typeof lastSync === 'number' ? new Date(lastSync).toLocaleTimeString() : 'never';
}

async function loadSessionMetadata(
  fallbackPlatform?: string,
  fallbackSessionStart?: number,
): Promise<number | null> {
  const stored = await chrome.storage.local.get([LAST_PLATFORM_KEY, SESSION_START_KEY]);
  const rawPlatform = fallbackPlatform ?? stored[LAST_PLATFORM_KEY] ?? null;
  const sessionStart =
    typeof fallbackSessionStart === 'number'
      ? fallbackSessionStart
      : typeof stored[SESSION_START_KEY] === 'number'
        ? stored[SESSION_START_KEY]
        : null;

  const platformEl = document.getElementById('platform');
  const durationEl = document.getElementById('sessionDuration');

  if (platformEl) {
    platformEl.textContent = formatPlatformLabel(rawPlatform);
  }

  if (durationEl) {
    durationEl.textContent = formatDuration(sessionStart);
  }

  return sessionStart;
}

function extractSignals(value: unknown): number[] {
  if (!value || typeof value !== 'object') return [];

  const rawSignals = (value as { signal_scores?: Record<string, number> }).signal_scores;
  if (!rawSignals || typeof rawSignals !== 'object') return [];

  return Object.values(rawSignals).filter((entry): entry is number => typeof entry === 'number');
}

async function loadRemoteScore(): Promise<PopupScoreState | null> {
  try {
    const data = await getScores();
    const firstScore = Array.isArray((data as { scores?: unknown[] }).scores)
      ? (data as { scores: unknown[] }).scores[0]
      : null;

    if (!firstScore || typeof firstScore !== 'object') {
      return null;
    }

    const scoreValue = typeof (firstScore as { score?: unknown }).score === 'number'
      ? (firstScore as { score: number }).score
      : 0;
    const levelValue = typeof (firstScore as { level?: unknown }).level === 'number'
      ? (firstScore as { level: number }).level
      : scoreToLevel(scoreValue);

    return {
      score: scoreValue,
      level: levelValue,
      signals: extractSignals(firstScore),
    };
  } catch {
    return null;
  }
}

async function initializePopup(): Promise<void> {
  renderSignalList(normalizeSignals([]));

  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  await Promise.all([loadAgentList(auth.authenticated), loadSyncStatus()]);

  const popupState = await requestPopupState();
  const sessionStart = await loadSessionMetadata(popupState.platform, popupState.sessionStart);

  let timerId: ReturnType<typeof setInterval> | null = null;
  if (sessionStart) {
    timerId = setInterval(() => {
      const durationEl = document.getElementById('sessionDuration');
      if (durationEl) {
        durationEl.textContent = formatDuration(sessionStart);
      } else if (timerId) {
        clearInterval(timerId);
      }
    }, 1000);
  }

  const remoteScore = auth.authenticated ? await loadRemoteScore() : null;
  const score = typeof popupState.score === 'number' ? popupState.score : 0;
  renderScoreState(remoteScore ?? {
    score,
    level: scoreToLevel(score),
    signals: [],
  });

  const currentAuth = getAuthState();
  if (currentAuth.authenticated !== auth.authenticated) {
    updateConnectionIndicator(currentAuth.authenticated);
  }
}

void initializePopup();
