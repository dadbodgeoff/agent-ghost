/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents, getScores } from '../background/gateway-client';

const LEVEL_LABELS = ['Level 0', 'Level 1', 'Level 2', 'Level 3', 'Level 4'];
const LEVEL_CLASSES = ['level-0', 'level-1', 'level-2', 'level-3', 'level-4'];
const SIGNAL_COUNT = 7;

interface PopupScorePayload {
  score: number;
  level: number;
  platform?: string;
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

function deriveLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

function normalizeSignals(signalScores: unknown): number[] {
  if (Array.isArray(signalScores)) {
    return signalScores.map((value) => (typeof value === 'number' ? value : 0));
  }
  if (signalScores && typeof signalScores === 'object') {
    return Object.values(signalScores)
      .map((value) => (typeof value === 'number' ? value : 0))
      .slice(0, SIGNAL_COUNT);
  }
  return [];
}

function normalizeScorePayload(raw: unknown): PopupScorePayload | null {
  if (!raw || typeof raw !== 'object') return null;

  const envelope = raw as Record<string, unknown>;
  const candidate =
    Array.isArray(envelope.scores) && envelope.scores.length > 0 && envelope.scores[0]
      ? (envelope.scores[0] as Record<string, unknown>)
      : envelope;

  const score =
    typeof candidate.score === 'number'
      ? candidate.score
      : typeof candidate.composite_score === 'number'
        ? candidate.composite_score
        : null;

  if (score === null) return null;

  return {
    score,
    level: typeof candidate.level === 'number' ? candidate.level : deriveLevel(score),
    platform:
      typeof candidate.platform === 'string'
        ? candidate.platform
        : typeof candidate.agent_name === 'string'
          ? candidate.agent_name
          : undefined,
    signals: normalizeSignals(candidate.signal_scores ?? candidate.signals),
  };
}

function signalColor(value: number): string {
  if (value < 0.3) return '#22c55e';
  if (value < 0.5) return '#eab308';
  if (value < 0.7) return '#f97316';
  return '#ef4444';
}

function renderSignalSkeleton(): void {
  const signalList = document.getElementById('signalList');
  if (!signalList) return;

  signalList.innerHTML = Array.from({ length: SIGNAL_COUNT }, (_, index) => `
    <div class="signal-row" role="listitem">
      <span class="signal-name">Signal ${index + 1}</span>
      <span class="signal-value" id="signal-value-${index}">0.000</span>
      <div class="signal-bar">
        <div class="signal-bar-fill" id="signal-bar-${index}" style="width:0%"></div>
      </div>
    </div>
  `).join('');
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
          `<span class="agent-state">${agent.state || agent.status || 'unknown'}</span>` +
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

async function persistSyncStatus(): Promise<void> {
  const ts = Date.now();
  await chrome.storage.local.set({ 'ghost-last-sync': ts });
  const el = document.getElementById('syncStatus');
  if (el) {
    el.textContent = new Date(ts).toLocaleTimeString();
  }
}

function updateUI(data: PopupScorePayload): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const platformEl = document.getElementById('platform');
  const alertEl = document.getElementById('alertBanner');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] || `Level ${data.level}`;
    levelEl.className = `level-badge ${LEVEL_CLASSES[data.level] || 'level-0'}`;
  }
  if (platformEl) {
    platformEl.textContent = data.platform ?? 'Gateway';
  }

  data.signals.forEach((value, index) => {
    const valueEl = document.getElementById(`signal-value-${index}`);
    const barEl = document.getElementById(`signal-bar-${index}`);
    if (valueEl) valueEl.textContent = value.toFixed(3);
    if (barEl) {
      (barEl as HTMLElement).style.width = `${(value * 100).toFixed(0)}%`;
      (barEl as HTMLElement).style.background = signalColor(value);
    }
  });

  if (!alertEl) return;
  if (data.level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      data.level >= 4
        ? 'Intervention Level 4 — External escalation active'
        : 'Intervention Level 3 — Session may be terminated';
  } else if (data.level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2 — Acknowledgment required';
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

function requestBackgroundScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') {
      return;
    }

    updateUI({
      score: response.score,
      level: deriveLevel(response.score),
      platform: 'Background monitor',
      signals: new Array(SIGNAL_COUNT).fill(0),
    });
  });
}

async function refreshGatewaySnapshot(): Promise<void> {
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);

  if (!auth.authenticated) {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
    return;
  }

  const scores = normalizeScorePayload(await getScores());
  if (scores) {
    updateUI(scores);
    await persistSyncStatus();
  } else {
    requestBackgroundScore();
  }

  await loadAgentList();
}

function startSessionTimer(): void {
  const sessionDuration = document.getElementById('sessionDuration');
  if (!sessionDuration) return;

  const sessionStart = Date.now();
  const render = () => {
    const elapsed = Math.floor((Date.now() - sessionStart) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    sessionDuration.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  render();
  setInterval(render, 1000);
}

document.addEventListener('DOMContentLoaded', async () => {
  renderSignalSkeleton();
  startSessionTimer();
  await loadSyncStatus();

  try {
    await initAuthSync();
    await refreshGatewaySnapshot();
  } catch {
    updateConnectionIndicator(false);
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Unable to load gateway state</span>';
    }
  }

  requestBackgroundScore();
});
