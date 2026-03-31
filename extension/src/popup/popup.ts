/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const signalLabels = [
  'Lexical overlap',
  'Response latency',
  'Hedging drop',
  'Persona drift',
  'Repetition',
  'Turn velocity',
  'Intervention pressure',
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

  container.innerHTML = '<span class="agent-list-empty">Loading agents...</span>';

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

function renderSignals(signals: number[]): void {
  const list = document.getElementById('signalList');
  if (!list) return;

  list.innerHTML = signalLabels
    .map((label, index) => {
      const value = signals[index] ?? 0;
      const percent = Math.max(0, Math.min(100, value * 100));
      const color = percent >= 70 ? '#ef4444' : percent >= 40 ? '#f59e0b' : '#22c55e';
      return `
        <div class="signal-row">
          <span class="signal-name">${label}</span>
          <span style="display:flex;align-items:center;">
            <span class="signal-value">${value.toFixed(2)}</span>
            <span class="signal-bar" aria-hidden="true">
              <span class="signal-bar-fill" style="width:${percent}%; background:${color}"></span>
            </span>
          </span>
        </div>
      `;
    })
    .join('');
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
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
      alertEl.className = `alert-banner active ${data.level >= 4 ? 'alert-danger' : 'alert-warning'}`;
    } else {
      alertEl.textContent = '';
      alertEl.className = 'alert-banner';
    }
  }
}

function updateSessionDuration(sessionStart: number): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) {
    timerEl.textContent = elapsed === 0 ? '<1m' : `${elapsed}m`;
  }
}

function updatePlatformLabel(gatewayUrl: string): void {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;

  try {
    platformEl.textContent = new URL(gatewayUrl).host;
  } catch {
    platformEl.textContent = gatewayUrl;
  }
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
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  updatePlatformLabel(auth.gatewayUrl);

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

chrome.runtime.onMessage.addListener((message) => {
  if (message?.type !== 'score_update' || typeof message.data !== 'number') {
    return;
  }

  const score = message.data;
  const level = score > 0.85 ? 4 :
                score > 0.7 ? 3 :
                score > 0.5 ? 2 :
                score > 0.3 ? 1 : 0;
  updateUI({
    score,
    level,
    signals: [0, 0, 0, 0, 0, 0, 0],
  });
});
