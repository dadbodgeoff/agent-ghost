/**
 * Popup script — displays convergence score and signals.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_LABELS = [
  'Linguistic fixation',
  'Rapid cadence',
  'Escalation risk',
  'Uncertainty drift',
  'Prompt instability',
  'Context overload',
  'Recovery lag',
];

interface PopupState {
  score: number;
  level: number;
  signals: number[];
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

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
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
      .map((a) => {
        const name = escapeHtml(a.name || a.id);
        const state = escapeHtml(a.state);
        return (
          `<div class="agent-list-item">` +
          `<span class="agent-name">${name}</span>` +
          `<span class="agent-state">${state}</span>` +
          `</div>`
        );
      })
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

function renderSignals(values: number[]): void {
  const signalList = document.getElementById('signalList');
  if (!signalList) return;

  signalList.innerHTML = SIGNAL_LABELS.map((label, index) => {
    const value = Math.max(0, Math.min(1, values[index] ?? 0));
    return `
      <div class="signal-row">
        <span class="signal-name">${escapeHtml(label)}</span>
        <div style="display:flex;align-items:center;">
          <span class="signal-value">${value.toFixed(2)}</span>
          <span class="signal-bar" aria-hidden="true">
            <span class="signal-bar-fill" style="width:${value * 100}%;background:${
              value >= 0.7 ? '#ef4444' : value >= 0.4 ? '#f59e0b' : '#22c55e'
            };"></span>
          </span>
        </div>
      </div>
    `;
  }).join('');
}

function updateUI(data: PopupState): void {
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
    alertEl.classList.remove('active', 'alert-warning', 'alert-danger');
    if (data.level >= 4) {
      alertEl.classList.add('active', 'alert-danger');
      alertEl.textContent = 'Convergence level 4 detected. Pause and review the session now.';
    } else if (data.level >= 3) {
      alertEl.classList.add('active', 'alert-warning');
      alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
    } else {
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

async function loadScore(): Promise<void> {
  try {
    const response = await chrome.runtime.sendMessage({ type: 'GET_SCORE' }) as { score?: number } | undefined;
    const score = typeof response?.score === 'number' ? response.score : 0;
    updateUI({
      score,
      level: deriveLevel(score),
      signals: [score, score * 0.82, score * 0.68, score * 0.55, score * 0.49, score * 0.37, score * 0.24],
    });
  } catch {
    updateUI({
      score: 0,
      level: 0,
      signals: [0, 0, 0, 0, 0, 0, 0],
    });
  }
}

// Session timer
const sessionStart = Date.now();
function updateSessionTimer(): void {
  const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
  const timerEl = document.getElementById('sessionDuration');
  if (timerEl) timerEl.textContent = `${elapsed}m`;
}

async function loadPlatform(): Promise<void> {
  const platformEl = document.getElementById('platform');
  if (!platformEl) return;

  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const hostname = tab?.url ? new URL(tab.url).hostname.replace(/^www\./, '') : null;
    platformEl.textContent = hostname ?? 'Unknown';
  } catch {
    platformEl.textContent = 'Unknown';
  }
}

// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
  updateSessionTimer();
  setInterval(updateSessionTimer, 60000);
  await loadPlatform();
  await loadScore();

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
