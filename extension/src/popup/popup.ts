/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_LABELS = [
  'Goal fixation',
  'Rapid agreement',
  'Style mirroring',
  'Escalation pressure',
  'Boundary erosion',
  'Tool overuse',
  'Loop repetition',
];

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
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

function renderSignals(signals: number[]): void {
  const list = document.getElementById('signalList');
  if (!list) return;

  const normalized = SIGNAL_LABELS.map((label, index) => ({
    label,
    value: Math.max(0, Math.min(1, signals[index] ?? 0)),
  }));

  list.innerHTML = normalized
    .map(
      ({ label, value }) => `
        <div class="signal-row">
          <span class="signal-name">${escapeHtml(label)}</span>
          <div style="display:flex;align-items:center;gap:8px;">
            <span class="signal-value">${value.toFixed(2)}</span>
            <div class="signal-bar" aria-hidden="true">
              <div class="signal-bar-fill" style="width:${value * 100}%;background:${
                value >= 0.7 ? '#ef4444' : value >= 0.4 ? '#f59e0b' : '#22c55e'
              };"></div>
            </div>
          </div>
        </div>
      `
    )
    .join('');
}

function updateSessionDuration(): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;

  const sessionStart = Number(sessionStorage.getItem('ghost-popup-session-start') ?? Date.now());
  if (!sessionStorage.getItem('ghost-popup-session-start')) {
    sessionStorage.setItem('ghost-popup-session-start', String(sessionStart));
  }

  const elapsedMinutes = Math.floor((Date.now() - sessionStart) / 60000);
  timerEl.textContent = elapsedMinutes <= 0 ? 'Just started' : `${elapsedMinutes}m`;
}

async function updatePlatformLabel(): Promise<void> {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;

  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const hostname = tab?.url ? new URL(tab.url).hostname.replace(/^www\./, '') : null;
    platformEl.textContent = hostname ?? 'Unavailable';
  } catch {
    platformEl.textContent = 'Unavailable';
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
          `<span class="agent-name">${escapeHtml(a.name || a.id)}</span>` +
          `<span class="agent-state">${escapeHtml(a.state)}</span>` +
          `</div>`
      )
      .join('');
  } catch {
    container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
  }
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

function updateUI(data: { score: number; level: number; signals: number[] }): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) scoreEl.textContent = data.score.toFixed(2);
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  renderSignals(data.signals);

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className =
        data.level >= 4 ? 'alert-banner active alert-danger' : 'alert-banner active alert-warning';
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function loadScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) {
      updateUI({ score: 0, level: 0, signals: [0, 0, 0, 0, 0, 0, 0] });
      return;
    }

    const rawScore = typeof response?.score === 'number' ? response.score : 0;
    const level =
      rawScore > 0.85 ? 4 :
      rawScore > 0.7 ? 3 :
      rawScore > 0.5 ? 2 :
      rawScore > 0.3 ? 1 : 0;

    updateUI({
      score: rawScore,
      level,
      signals: [rawScore, rawScore * 0.8, rawScore * 0.65, rawScore * 0.5, rawScore * 0.35, rawScore * 0.2, rawScore * 0.1],
    });
  });
}

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);
  loadScore();
  updateSessionDuration();
  updatePlatformLabel();

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
  }

  await loadSyncStatus();
})();

setInterval(updateSessionDuration, 60000);
