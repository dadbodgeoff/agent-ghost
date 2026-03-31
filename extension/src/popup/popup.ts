/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents, getScores } from '../background/gateway-client';

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
          `<span class="agent-state">${a.state || 'unknown'}</span>` +
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

  const signalIds = ['s1', 's2', 's3', 's4', 's5', 's6', 's7'];
  data.signals.forEach((val, i) => {
    const el = document.getElementById(signalIds[i]);
    if (el) el.textContent = val.toFixed(2);
  });

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

async function loadScores(): Promise<void> {
  try {
    const data = await getScores();
    const scoreList = Array.isArray(data.scores) ? data.scores : [];
    const scoreEntry =
      scoreList[0] && typeof scoreList[0] === 'object'
        ? (scoreList[0] as Record<string, unknown>)
        : undefined;
    const score =
      scoreEntry && typeof scoreEntry.score === 'number'
        ? scoreEntry.score
        : 0;
    const level =
      scoreEntry && typeof scoreEntry.level === 'number'
        ? scoreEntry.level
        : score > 0.85
          ? 4
          : score > 0.7
            ? 3
            : score > 0.5
              ? 2
              : score > 0.3
                ? 1
                : 0;

    updateUI({
      score,
      level,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
    return;
  } catch {
    // Fall back to the in-memory background score if the gateway call fails.
  }

  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (response && typeof response.score === 'number') {
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
}

// Session timer
const sessionStart = Date.now();
function renderSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

renderSessionTimer();
setInterval(renderSessionTimer, 60000);

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    platformEl.textContent = auth.gatewayUrl;
  }

  if (auth.authenticated) {
    await loadScores();
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
    updateUI({ score: 0, level: 0, signals: [0, 0, 0, 0, 0, 0, 0] });
  }

  await loadSyncStatus();
})();
