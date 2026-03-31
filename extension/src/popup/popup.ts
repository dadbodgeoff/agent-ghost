/**
 * Popup script — displays convergence score, signals, and gateway state.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents, getScores } from '../background/gateway-client';

const FALLBACK_SIGNAL_NAMES = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
];

const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'];

type PopupData = {
  score: number;
  level: number;
  signalEntries: Array<[string, number]>;
  platform?: string;
};

type GatewayScoreEntry = {
  agent_name?: string;
  level?: number;
  score?: number;
  signal_scores?: Record<string, number>;
};

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

function levelLabel(level: number): string {
  return `Level ${Math.max(0, Math.min(4, level))}`;
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function formatSignalName(name: string): string {
  return name
    .split('_')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function renderSignals(signalEntries: Array<[string, number]>): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  const entries =
    signalEntries.length > 0
      ? signalEntries
      : FALLBACK_SIGNAL_NAMES.map((name) => [name, 0] as [string, number]);

  container.innerHTML = entries
    .map(
      ([name, value], index) => `
        <div class="signal-row" role="listitem">
          <span class="signal-name">${formatSignalName(name)}</span>
          <span class="signal-value" id="signal-value-${index}">${value.toFixed(3)}</span>
          <div class="signal-bar">
            <div
              class="signal-bar-fill"
              id="signal-bar-${index}"
              style="width:${(Math.max(0, Math.min(1, value)) * 100).toFixed(0)}%; background:${scoreColor(value)}"
            ></div>
          </div>
        </div>
      `,
    )
    .join('');
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

function updateUI(data: PopupData): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const platformEl = document.getElementById('platform');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.score);
  }

  if (levelEl) {
    levelEl.textContent = levelLabel(data.level);
    levelEl.className = `level-badge ${LEVEL_CLASSES[data.level] ?? LEVEL_CLASSES[0]}`;
  }

  renderSignals(data.signalEntries);

  if (platformEl) {
    platformEl.textContent = data.platform ?? '—';
  }

  if (!alertEl) return;

  if (data.level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      data.level >= 4
        ? 'Intervention Level 4 — external escalation active'
        : 'Intervention Level 3 — session may require termination';
  } else if (data.level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2 — acknowledgment recommended';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

async function loadScoreCard(): Promise<void> {
  try {
    const result = await getScores();
    const scores = Array.isArray(result.scores) ? (result.scores as GatewayScoreEntry[]) : [];
    const firstScore = scores[0];

    if (!firstScore) {
      updateUI({ score: 0, level: 0, signalEntries: [] });
      return;
    }

    updateUI({
      score: firstScore.score ?? 0,
      level: firstScore.level ?? 0,
      signalEntries: Object.entries(firstScore.signal_scores ?? {}),
      platform: firstScore.agent_name || undefined,
    });
  } catch {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
      if (!response || typeof response.score !== 'number') return;

      const level =
        response.score > 0.85 ? 4 :
        response.score > 0.7 ? 3 :
        response.score > 0.5 ? 2 :
        response.score > 0.3 ? 1 : 0;

      updateUI({
        score: response.score,
        level,
        signalEntries: [],
      });
    });
  }
}

const sessionStart = Date.now();

function updateSessionDuration(): void {
  const elapsedSeconds = Math.floor((Date.now() - sessionStart) / 1000);
  const hours = Math.floor(elapsedSeconds / 3600);
  const minutes = Math.floor((elapsedSeconds % 3600) / 60);
  const seconds = elapsedSeconds % 60;
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
}

updateSessionDuration();
setInterval(updateSessionDuration, 1000);

renderSignals([]);

(async () => {
  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (auth.authenticated) {
    await Promise.all([loadAgentList(), loadScoreCard()]);
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
    updateUI({ score: 0, level: 0, signalEntries: [] });
  }

  await loadSyncStatus();
})();
