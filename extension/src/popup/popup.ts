/**
 * Popup script — displays convergence score, gateway connectivity, and agent state.
 */

import { initAuthSync } from '../background/auth-sync';
import { getAgents } from '../background/gateway-client';

const SIGNAL_NAMES = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
];

const LEVEL_LABELS = [
  'Level 0 - Normal',
  'Level 1 - Soft',
  'Level 2 - Active',
  'Level 3 - Hard',
  'Level 4 - External',
];

let sessionStart = Date.now();

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

function renderSignalList(values: number[] = Array(SIGNAL_NAMES.length).fill(0)): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map((name, index) => {
    const value = Number.isFinite(values[index]) ? values[index] : 0;
    const width = `${Math.max(0, Math.min(100, value * 100))}%`;
    return `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value">${value.toFixed(3)}</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" style="width:${width};background:${scoreColor(value)}"></div>
        </div>
      </div>
    `;
  }).join('');
}

function updateScore(score: number, level: number): void {
  const scoreEl = document.getElementById('scoreValue');
  const badge = document.getElementById('levelBadge');

  if (scoreEl) {
    scoreEl.textContent = Number.isFinite(score) ? score.toFixed(2) : '—';
    scoreEl.setAttribute('aria-label', `Convergence score ${Number.isFinite(score) ? score.toFixed(2) : 'unavailable'}`);
    (scoreEl as HTMLElement).style.color = Number.isFinite(score) ? scoreColor(score) : '';
  }

  if (badge) {
    const normalizedLevel = Math.max(0, Math.min(LEVEL_LABELS.length - 1, level || 0));
    badge.textContent = LEVEL_LABELS[normalizedLevel] ?? `Level ${normalizedLevel}`;
    badge.className = `level-badge level-${normalizedLevel}`;
  }

  updateAlert(level);
}

function updateAlert(level: number): void {
  const banner = document.getElementById('alertBanner');
  if (!banner) return;

  banner.className = 'alert-banner';
  banner.textContent = '';

  if (level >= 3) {
    banner.classList.add('active', 'alert-danger');
    banner.textContent = level >= 4
      ? 'Intervention Level 4 - External escalation active.'
      : 'Intervention Level 3 - Session may require immediate intervention.';
    return;
  }

  if (level >= 2) {
    banner.classList.add('active', 'alert-warning');
    banner.textContent = 'Intervention Level 2 - Elevated convergence detected.';
  }
}

function updatePlatformLabel(label: string): void {
  const platform = document.getElementById('platform');
  if (platform) {
    platform.textContent = label;
  }
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

    container.innerHTML = agents.map((agent) => `
      <div class="agent-list-item">
        <span class="agent-name">${agent.name || agent.id}</span>
        <span class="agent-state">${agent.state}</span>
      </div>
    `).join('');
  } catch {
    container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
  }
}

async function loadSyncStatus(): Promise<void> {
  const el = document.getElementById('syncStatus');
  if (!el) return;

  const stored = await chrome.storage.local.get('ghost-last-sync');
  const timestamp = stored['ghost-last-sync'];
  el.textContent = typeof timestamp === 'number' ? new Date(timestamp).toLocaleTimeString() : 'never';
}

function startSessionTimer(): void {
  const el = document.getElementById('sessionDuration');
  if (!el) return;

  sessionStart = Date.now();
  const render = () => {
    const elapsed = Math.floor((Date.now() - sessionStart) / 1000);
    const hours = Math.floor(elapsed / 3600);
    const minutes = Math.floor((elapsed % 3600) / 60);
    const seconds = elapsed % 60;
    el.textContent = `${hours}h ${minutes}m ${seconds}s`;
  };

  render();
  window.setInterval(render, 1000);
}

function requestScore(): void {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') {
      updateScore(Number.NaN, 0);
      renderSignalList();
      return;
    }

    const score = response.score;
    const level = score > 0.85 ? 4 :
      score > 0.7 ? 3 :
      score > 0.5 ? 2 :
      score > 0.3 ? 1 : 0;

    updateScore(score, level);
    renderSignalList(Array(SIGNAL_NAMES.length).fill(score));
  });
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

async function initPopup(): Promise<void> {
  renderSignalList();
  startSessionTimer();
  requestScore();

  const auth = await initAuthSync();
  updateConnectionIndicator(auth.authenticated);
  updatePlatformLabel(auth.authenticated ? new URL(auth.gatewayUrl).host : 'Not connected');

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
