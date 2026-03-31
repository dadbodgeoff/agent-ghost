/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState, initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_LABELS = [
  'Velocity',
  'Repetition',
  'Dwell',
  'Context',
  'Escalation',
  'Recovery',
  'Novelty',
];

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

  const signalList = document.getElementById('signalList');
  if (signalList) {
    signalList.innerHTML = data.signals
      .map((value, index) => {
        const percent = Math.max(0, Math.min(100, Math.round(value * 100)));
        return (
          `<div class="signal-row">` +
          `<span class="signal-name">${SIGNAL_LABELS[index] ?? `Signal ${index + 1}`}</span>` +
          `<span style="display:flex;align-items:center;gap:8px;">` +
          `<span class="signal-value">${value.toFixed(2)}</span>` +
          `<span class="signal-bar"><span class="signal-bar-fill" style="width:${percent}%;background:${signalBarColor(data.level)};"></span></span>` +
          `</span>` +
          `</div>`
        );
      })
      .join('');
  }

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = `alert-banner active ${data.level >= 4 ? 'alert-danger' : 'alert-warning'}`;
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function signalBarColor(level: number): string {
  if (level >= 4) return '#f87171';
  if (level >= 3) return '#fbbf24';
  if (level >= 2) return '#fb923c';
  if (level >= 1) return '#fde047';
  return '#22c55e';
}

function updateSessionDuration(sessionStart: number): void {
  const elapsedMinutes = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) {
    timerEl.textContent = `${elapsedMinutes}m`;
  }
}

function updatePlatformLabel(): void {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('firefox')) {
    platformEl.textContent = 'Firefox';
    return;
  }
  if (ua.includes('edg/')) {
    platformEl.textContent = 'Edge';
    return;
  }
  platformEl.textContent = 'Chrome';
}

// Request score from background
chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (response && response.score !== undefined) {
    const level = response.score > 0.85 ? 4 :
                  response.score > 0.7 ? 3 :
                  response.score > 0.5 ? 2 :
                  response.score > 0.3 ? 1 : 0;
    updateUI({
      score: response.score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  }
});

// Session timer
const sessionStart = Date.now();
updateSessionDuration(sessionStart);
setInterval(() => {
  updateSessionDuration(sessionStart);
}, 60000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  await initAuthSync();
  const auth = getAuthState();
  updateConnectionIndicator(auth.authenticated);
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
