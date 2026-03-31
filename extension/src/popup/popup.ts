/**
 * Popup script — displays convergence status, alerting, and agent connectivity.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';
import { updateAlertBanner } from './components/AlertBanner';
import { renderScoreGauge, updateScoreGauge } from './components/ScoreGauge';
import { renderSignalList, updateSignalList } from './components/SignalList';
import { startSessionTimer } from './components/SessionTimer';

interface PopupMetrics {
  score: number;
  level: number;
  signals: number[];
}

const DEFAULT_SIGNALS = [0, 0, 0, 0, 0, 0, 0];

function getLevel(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

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

function renderAgentMessage(message: string): void {
  const container = document.getElementById('agentList');
  if (!container) return;
  container.innerHTML = `<span class="agent-list-empty">${message}</span>`;
}

async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  try {
    const agents = await getAgents();
    if (agents.length === 0) {
      renderAgentMessage('No agents found');
      return;
    }

    container.innerHTML = agents
      .map(
        (agent) =>
          `<div class="agent-list-item">` +
          `<span class="agent-name">${agent.name || agent.id}</span>` +
          `<span class="agent-state">${agent.state}</span>` +
          `</div>`
      )
      .join('');
  } catch {
    renderAgentMessage('Unable to load agents');
  }
}

async function loadSyncStatus(): Promise<void> {
  const element = document.getElementById('syncStatus');
  if (!element) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const timestamp = stored['ghost-last-sync'];
  element.textContent =
    typeof timestamp === 'number' ? new Date(timestamp).toLocaleTimeString() : 'never';
}

function updatePlatform(authenticated: boolean, gatewayUrl: string): void {
  const platform = document.getElementById('platform');
  if (!platform) return;

  if (!authenticated) {
    platform.textContent = 'Offline';
    return;
  }

  try {
    const { hostname } = new URL(gatewayUrl);
    platform.textContent = hostname || gatewayUrl;
  } catch {
    platform.textContent = gatewayUrl;
  }
}

function updateMetrics(data: PopupMetrics): void {
  const scoreContainer = document.getElementById('scoreGauge');
  const alertBanner = document.getElementById('alertBanner');

  if (scoreContainer) {
    if (scoreContainer.querySelector('.score-value')) {
      updateScoreGauge(scoreContainer, data.score, data.level);
    } else {
      renderScoreGauge(scoreContainer, data.score, data.level);
    }
  }

  const badge = document.getElementById('levelBadge');
  if (badge) {
    badge.textContent = `Level ${data.level}`;
    badge.className = `level-badge level-${data.level}`;
  }

  updateSignalList(data.signals);

  if (alertBanner) {
    updateAlertBanner(alertBanner, data.level);
  }
}

function requestScore(): Promise<void> {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
      if (chrome.runtime.lastError || response?.score === undefined) {
        resolve();
        return;
      }

      const score = Number(response.score);
      if (!Number.isFinite(score)) {
        resolve();
        return;
      }

      updateMetrics({
        score,
        level: getLevel(score),
        signals: DEFAULT_SIGNALS,
      });
      resolve();
    });
  });
}

async function initPopup(): Promise<void> {
  const scoreContainer = document.getElementById('scoreGauge');
  const sessionDuration = document.getElementById('sessionDuration');

  if (scoreContainer) {
    renderScoreGauge(scoreContainer, 0, 0);
  }

  const levelBadge = document.getElementById('levelBadge');
  if (levelBadge) {
    levelBadge.textContent = 'Level 0';
  }

  const signalList = document.getElementById('signalList');
  if (signalList) {
    renderSignalList(signalList);
    updateSignalList(DEFAULT_SIGNALS);
  }

  const alertBanner = document.getElementById('alertBanner');
  if (alertBanner) {
    updateAlertBanner(alertBanner, 0);
  }

  if (sessionDuration) {
    startSessionTimer(sessionDuration);
  }

  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  updatePlatform(auth.authenticated, auth.gatewayUrl);

  if (auth.authenticated) {
    await Promise.all([loadAgentList(), requestScore()]);
  } else {
    renderAgentMessage('Not connected to gateway');
  }

  await loadSyncStatus();
}

void initPopup();
