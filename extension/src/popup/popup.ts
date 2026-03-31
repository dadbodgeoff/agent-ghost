/**
 * Popup script — displays convergence score and signals.
 */

interface AuthState {
  authenticated: boolean;
}

interface AgentSummary {
  id: string;
  name: string;
  state: string;
}

const SIGNAL_LABELS = [
  'Novelty',
  'Looping',
  'Velocity',
  'Coherence',
  'Risk',
  'Escalation',
  'Recovery',
];
const popupOpenedAt = Date.now();

function sendRuntimeMessage<T>(message: Record<string, unknown>): Promise<T> {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendMessage(message, (response) => {
      const runtimeError = chrome.runtime.lastError;
      if (runtimeError) {
        reject(new Error(runtimeError.message));
        return;
      }
      resolve(response as T);
    });
  });
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

/**
 * Fetch and render the agent list from the gateway.
 */
async function loadAgentList(connected: boolean): Promise<void> {
  const container = document.getElementById('agentList');
  if (!container) return;

  if (!connected) {
    container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
    return;
  }

  try {
    const response = await sendRuntimeMessage<{ agents?: AgentSummary[]; error?: string }>({
      type: 'GET_AGENTS',
    });
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
          `</div>`,
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

function renderSignals(signals: number[]): void {
  const container = document.getElementById('signalList');
  if (!container) return;

  container.innerHTML = SIGNAL_LABELS.map((label, index) => {
    const rawValue = signals[index] ?? 0;
    const value = Math.max(0, Math.min(1, rawValue));
    const color = value >= 0.7 ? '#ef4444' : value >= 0.4 ? '#f59e0b' : '#22c55e';
    return (
      `<div class="signal-row">` +
      `<span class="signal-name">${label}</span>` +
      `<span style="display:flex;align-items:center;">` +
      `<span class="signal-value">${value.toFixed(2)}</span>` +
      `<span class="signal-bar"><span class="signal-bar-fill" style="width:${value * 100}%;background:${color};"></span></span>` +
      `</span>` +
      `</div>`
    );
  }).join('');
}

function renderSessionDuration(): void {
  const el = document.getElementById('sessionDuration');
  if (!el) return;

  const elapsedMinutes = Math.floor((Date.now() - popupOpenedAt) / 60000);
  el.textContent = `${elapsedMinutes}m`;
}

async function populatePlatform(): Promise<void> {
  const el = document.getElementById('platform');
  if (!el) return;

  try {
    const [activeTab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (!activeTab?.url) {
      el.textContent = 'Unavailable';
      return;
    }

    el.textContent = new URL(activeTab.url).hostname.replace(/^www\./, '');
  } catch {
    el.textContent = 'Unavailable';
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

  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;

  if (data.level >= 3) {
    alertEl.className = `alert-banner active ${data.level >= 4 ? 'alert-danger' : 'alert-warning'}`;
    alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
  } else {
    alertEl.className = 'alert-banner';
    alertEl.textContent = '';
  }
}

async function loadScore(): Promise<void> {
  try {
    const response = await sendRuntimeMessage<{ score?: number }>({ type: 'GET_SCORE' });
    const score = response.score ?? 0;
    const level = score > 0.85 ? 4 :
      score > 0.7 ? 3 :
      score > 0.5 ? 2 :
      score > 0.3 ? 1 : 0;
    updateUI({
      score,
      level,
      signals: new Array(SIGNAL_LABELS.length).fill(score),
    });
  } catch {
    updateUI({
      score: 0,
      level: 0,
      signals: new Array(SIGNAL_LABELS.length).fill(0),
    });
  }
}

async function initPopup(): Promise<void> {
  renderSessionDuration();
  await populatePlatform();

  const response = await sendRuntimeMessage<{ auth: AuthState }>({ type: 'GET_AUTH_STATE' });
  const auth = response.auth;
  updateConnectionIndicator(auth.authenticated);
  await loadAgentList(auth.authenticated);
  await loadSyncStatus();
  await loadScore();
}

setInterval(renderSessionDuration, 60_000);
void initPopup();
