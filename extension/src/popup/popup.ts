/**
 * Popup script — displays convergence score and signals.
 */

const SIGNAL_NAMES = [
  'Session Duration',
  'Inter-Session Gap',
  'Response Latency',
  'Vocabulary Convergence',
  'Goal Boundary Erosion',
  'Initiative Balance',
  'Disengagement Resistance',
];

type PopupStatus = {
  connected: boolean;
  authenticated: boolean;
  gatewayUrl?: string;
  lastValidated?: number;
  latestScore?: {
    composite_score?: number;
    level?: number;
    signals?: number[];
    platform?: string;
  } | null;
};

type AgentListResponse = {
  agents?: Array<{ id: string; name?: string; state?: string }>;
  error?: string;
};

function sendRuntimeMessage<T>(type: string): Promise<T> {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendMessage({ type }, (response: T | undefined) => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
        return;
      }
      if (response === undefined) {
        reject(new Error(`No response for ${type}`));
        return;
      }
      resolve(response);
    });
  });
}

function renderSignalList(): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_NAMES.map(
    (name, i) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${i}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${i}" style="width:0%"></div>
        </div>
      </div>
    `,
  ).join('');
}

function scoreColor(score: number): string {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

/**
 * Update the connection indicator (statusDot + statusLabel).
 */
function updateConnectionIndicator(connected: boolean, labelText?: string): void {
  const dot = document.getElementById('statusDot');
  const label = document.getElementById('statusLabel');
  if (dot) {
    dot.classList.remove('connected', 'disconnected');
    dot.classList.add(connected ? 'connected' : 'disconnected');
  }
  if (label) {
    label.classList.remove('connected', 'disconnected');
    label.classList.add(connected ? 'connected' : 'disconnected');
    label.textContent = labelText ?? (connected ? 'Connected' : 'Disconnected');
  }
}

/**
 * Fetch and render the agent list from the gateway.
 */
async function loadAgentList(): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  try {
    const response = await sendRuntimeMessage<AgentListResponse>('GET_AGENTS');
    const agents = response.agents ?? [];
    if (agents.length === 0) {
      container.innerHTML = `<span class="agent-list-empty">${response.error ?? 'No agents found'}</span>`;
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

  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    (scoreEl as HTMLElement).style.color = scoreColor(data.score);
  }
  if (levelEl) {
    levelEl.textContent = `Level ${data.level}`;
    levelEl.className = `level-badge level-${data.level}`;
  }

  data.signals.forEach((val, i) => {
    const valueEl = document.getElementById(`signal-value-${i}`);
    const barEl = document.getElementById(`signal-bar-${i}`) as HTMLElement | null;
    if (valueEl) {
      valueEl.textContent = val.toFixed(3);
    }
    if (barEl) {
      barEl.style.width = `${Math.max(0, Math.min(100, val * 100))}%`;
      barEl.style.background = scoreColor(val);
    }
  });

  const alertEl = document.getElementById('alertBanner');
  if (alertEl) {
    if (data.level >= 3) {
      alertEl.className = 'alert-banner active alert-danger';
      alertEl.textContent = `Intervention Level ${data.level} detected. Consider taking a break.`;
    } else if (data.level >= 2) {
      alertEl.className = 'alert-banner active alert-warning';
      alertEl.textContent = 'Intervention Level 2 detected. Acknowledgment recommended.';
    } else {
      alertEl.className = 'alert-banner';
      alertEl.textContent = '';
    }
  }
}

function updateSessionTimer(startedAt: number): void {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;
  const elapsed = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
  const mins = Math.floor(elapsed / 60);
  const secs = elapsed % 60;
  timerEl.textContent = `${mins}m ${secs}s`;
}

async function refreshPopup(): Promise<void> {
  try {
    const status = await sendRuntimeMessage<PopupStatus>('GET_STATUS');
    updateConnectionIndicator(status.connected, status.connected ? 'Connected' : 'Offline');
    if (status.latestScore) {
      updateUI({
        score: status.latestScore.composite_score ?? 0,
        level: status.latestScore.level ?? 0,
        signals: status.latestScore.signals ?? [0, 0, 0, 0, 0, 0, 0],
      });
    }

    const platformEl = document.getElementById('platform');
    if (platformEl) {
      platformEl.textContent = status.gatewayUrl || 'Native monitor';
    }

    if (status.authenticated) {
      await loadAgentList();
    } else {
      const container = document.getElementById('agentList');
      if (container) {
        container.innerHTML = '<span class="agent-list-empty">Authenticate with the gateway to load agents</span>';
      }
    }
  } catch {
    updateConnectionIndicator(false, 'Unavailable');
    const container = document.getElementById('agentList');
    if (container) {
      container.innerHTML = '<span class="agent-list-empty">Background service unavailable</span>';
    }
  }

  await loadSyncStatus();
}

// Session timer
const sessionStart = Date.now();
updateSessionTimer(sessionStart);
setInterval(() => {
  updateSessionTimer(sessionStart);
}, 1000);

document.addEventListener('DOMContentLoaded', () => {
  renderSignalList();
  void refreshPopup();
  setInterval(() => {
    void refreshPopup();
  }, 5000);
});
