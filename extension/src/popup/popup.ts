/**
 * Popup script — displays convergence score, signals, and gateway connectivity.
 */

import { ensureAuthSync, getAuthState } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { updateAlertBanner } from './components/AlertBanner';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer } from './components/SessionTimer';

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

function updateScore(score: number, level: number): void {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');

  if (scoreEl) {
    scoreEl.textContent = score.toFixed(2);
  }

  if (levelEl) {
    levelEl.textContent = `Level ${level}`;
    levelEl.className = `level-badge level-${level}`;
  }

  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    updateAlertBanner(alertEl, level);
  }
}

function deriveLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
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
  } catch (error) {
    container.innerHTML = `<span class="agent-list-empty">${
      error instanceof Error ? error.message : 'Unable to load agents'
    }</span>`;
  }
}

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

function initSignals(): void {
  const signalList = document.getElementById('signalList');
  if (!signalList) return;
  renderSignalList(signalList);
  updateSignalList([0, 0, 0, 0, 0, 0, 0]);
}

function initSessionInfo(): void {
  const sessionDuration = document.getElementById('sessionDuration');
  if (sessionDuration) {
    startSessionTimer(sessionDuration);
  }

  const platform = document.getElementById('platform');
  if (platform) {
    platform.textContent = 'Browser extension';
  }
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError) {
      return;
    }

    const score = typeof response?.score === 'number' ? response.score : 0;
    updateScore(score, deriveLevel(score));
  });
}

async function initPopup(): Promise<void> {
  initSignals();
  initSessionInfo();
  requestScore();

  await ensureAuthSync();
  const auth = getAuthState();
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
}

void initPopup();
