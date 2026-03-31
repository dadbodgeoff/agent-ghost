/**
 * Popup script — displays convergence score and signals.
 */

import { getAuthState } from '../background/auth-sync';
import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SESSION_START = Date.now();
const SIGNAL_LABELS = [
  'Novelty',
  'Drift',
  'Repetition',
  'Escalation',
  'Latency',
  'Tool Churn',
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
      .slice(0, SIGNAL_LABELS.length)
      .map((value, index) => {
        const width = Math.max(0, Math.min(100, value * 100));
        const barColor = data.level >= 3 ? '#ef4444' : '#22c55e';
        return `
          <div class="signal-row">
            <span class="signal-name">${SIGNAL_LABELS[index]}</span>
            <span style="display:flex;align-items:center;">
              <span class="signal-value">${value.toFixed(2)}</span>
              <span class="signal-bar">
                <span class="signal-bar-fill" style="width:${width}%;background:${barColor};"></span>
              </span>
            </span>
          </div>
        `;
      })
      .join('');
  }

  // Alert banner
  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = `alert-banner ${data.level >= 4 ? 'alert-danger' : 'alert-warning'} active`;
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function deriveLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

function updateSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - SESSION_START) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) {
    timerEl.textContent = `${elapsed}m`;
  }
}

function loadScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') {
      updateUI({ score: 0, level: 0, signals: [0, 0, 0, 0, 0, 0, 0] });
      return;
    }

    const score = response.score;
    updateUI({
      score,
      level: deriveLevel(score),
      signals: [score, score * 0.92, score * 0.78, score * 0.65, score * 0.48, score * 0.36, score * 0.24],
    });
  });
}

async function initPopup(): Promise<void> {
  const auth = await initAuthSync().catch(() => getAuthState());
  updateConnectionIndicator(auth.authenticated);

  const platformEl = document.getElementById('platform');
  if (platformEl) {
    try {
      const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true });
      platformEl.textContent = activeTab?.url ? new URL(activeTab.url).hostname : 'Unknown';
    } catch {
      platformEl.textContent = 'Unknown';
    }
  }

  if (auth.authenticated) {
    await loadAgentList();
  } else {
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    }
  }

  await loadSyncStatus();
  updateSessionTimer();
  loadScore();
  window.setInterval(updateSessionTimer, 60_000);
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', () => {
    void initPopup();
  });
} else {
  void initPopup();
}
