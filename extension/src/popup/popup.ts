/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_LABELS = [
  'Novelty',
  'Latency',
  'Session drift',
  'Repetition',
  'Tool churn',
  'Interruptions',
  'Recovery',
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

function renderSignals(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_LABELS.map((label, index) => {
    const value = signals[index] ?? 0;
    const clamped = Math.max(0, Math.min(1, value));
    const percent = Math.round(clamped * 100);
    return (
      `<div class="signal-row">` +
      `<span class="signal-name">${label}</span>` +
      `<span style="display:flex;align-items:center;">` +
      `<span class="signal-value">${value.toFixed(2)}</span>` +
      `<span class="signal-bar" aria-hidden="true">` +
      `<span class="signal-bar-fill" style="width:${percent}%;background:${percent >= 75 ? '#ef4444' : percent >= 45 ? '#f59e0b' : '#22c55e'};"></span>` +
      `</span>` +
      `</span>` +
      `</div>`
    );
  }).join('');
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
        (a) => {
          const state = a.effective_state ?? a.status ?? 'unknown';
          return (
          `<div class="agent-list-item">` +
          `<span class="agent-name">${a.name || a.id}</span>` +
          `<span class="agent-state">${state.replaceAll('_', ' ')}</span>` +
          `</div>`
          );
        }
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
      alertEl.className = `alert-banner active ${data.level >= 4 ? 'alert-danger' : 'alert-warning'}`;
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

// Request score from background
chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
  if (chrome.runtime.lastError) return;
  if (!response || response.score === undefined) return;

  const level = response.score > 0.85 ? 4 :
                response.score > 0.7 ? 3 :
                response.score > 0.5 ? 2 :
                response.score > 0.3 ? 1 : 0;
  updateUI({
    score: response.score,
    level,
    signals: [0, 0, 0, 0, 0, 0, 0],
  });
});

// Session timer
const sessionStart = Date.now();
function updateSessionDuration(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

updateSessionDuration();
setInterval(updateSessionDuration, 60000);

const platformEl = document.getElementById('platform');
if (platformEl) {
  platformEl.textContent = 'Browser extension';
}

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

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
