/**
 * Popup script — displays convergence score and signals.
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
/**
 * Update the connection indicator (statusDot + statusLabel).
 */
function updateConnectionIndicator(connected) {
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
function renderSignalList() {
    const container = document.getElementById('signalList');
    if (!container)
        return;
    container.innerHTML = SIGNAL_NAMES.map((name, index) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${index}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="signal-bar-${index}" style="width: 0%"></div>
        </div>
      </div>
    `).join('');
}
function scoreColor(score) {
    if (score < 0.3)
        return '#22c55e';
    if (score < 0.5)
        return '#eab308';
    if (score < 0.7)
        return '#f97316';
    return '#ef4444';
}
/**
 * Fetch and render the agent list from the gateway.
 */
async function loadAgentList() {
    const container = document.getElementById('agentList');
    if (!container)
        return;
    try {
        const agents = await getAgents();
        if (agents.length === 0) {
            container.innerHTML = '<span class="agent-list-empty">No agents found</span>';
            return;
        }
        container.innerHTML = agents
            .map((a) => `<div class="agent-list-item">` +
            `<span class="agent-name">${a.name || a.id}</span>` +
            `<span class="agent-state">${a.state}</span>` +
            `</div>`)
            .join('');
    }
    catch {
        container.innerHTML = '<span class="agent-list-empty">Unable to load agents</span>';
    }
}
/**
 * Load and display the last sync time from storage.
 */
async function loadSyncStatus() {
    const el = document.getElementById('syncStatus');
    if (!el)
        return;
    const stored = await chrome.storage.local.get('ghost-last-sync');
    const ts = stored['ghost-last-sync'];
    if (ts && typeof ts === 'number') {
        el.textContent = new Date(ts).toLocaleTimeString();
    }
    else {
        el.textContent = 'never';
    }
}
function updateUI(data) {
    const scoreEl = document.getElementById('scoreValue');
    const levelEl = document.getElementById('levelBadge');
    if (scoreEl) {
        scoreEl.textContent = data.score.toFixed(2);
        scoreEl.style.color = scoreColor(data.score);
    }
    if (levelEl) {
        levelEl.textContent = LEVEL_LABELS[data.level] ?? `Level ${data.level}`;
        levelEl.className = `level-badge level-${data.level}`;
    }
    data.signals.forEach((val, i) => {
        const valueEl = document.getElementById(`signal-value-${i}`);
        const barEl = document.getElementById(`signal-bar-${i}`);
        if (valueEl) {
            valueEl.textContent = val.toFixed(3);
        }
        if (barEl) {
            barEl.style.width = `${Math.max(0, Math.min(val, 1)) * 100}%`;
            barEl.style.background = scoreColor(val);
        }
    });
    const alertEl = document.getElementById('alertBanner');
    if (alertEl) {
        if (data.level >= 3) {
            alertEl.className = 'alert-banner active alert-danger';
            alertEl.textContent =
                data.level >= 4
                    ? 'Intervention Level 4 - External escalation active'
                    : 'Intervention Level 3 - Session may be terminated';
        }
        else if (data.level >= 2) {
            alertEl.className = 'alert-banner active alert-warning';
            alertEl.textContent = 'Intervention Level 2 - Acknowledgment required';
        }
        else {
            alertEl.className = 'alert-banner';
            alertEl.textContent = '';
        }
    }
}
function refreshScore() {
    chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
        if (chrome.runtime.lastError || !response || response.score === undefined) {
            return;
        }
        const score = Number(response.score) || 0;
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
}
function startSessionTimer() {
    const sessionStart = Date.now();
    const timerEl = document.getElementById('sessionDuration');
    if (!timerEl)
        return;
    const render = () => {
        const elapsedSeconds = Math.floor((Date.now() - sessionStart) / 1000);
        const hours = Math.floor(elapsedSeconds / 3600);
        const minutes = Math.floor((elapsedSeconds % 3600) / 60);
        const seconds = elapsedSeconds % 60;
        timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
    };
    render();
    window.setInterval(render, 1000);
}
async function initPopup() {
    renderSignalList();
    startSessionTimer();
    refreshScore();
    await loadSyncStatus();
    const platformEl = document.getElementById('platform');
    if (platformEl) {
        platformEl.textContent = 'Browser extension';
    }
    const auth = await initAuthSync();
    updateConnectionIndicator(auth.authenticated);
    if (!auth.authenticated) {
        const container = document.getElementById('agentList');
        if (container) {
            container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
        }
        return;
    }
    await loadAgentList();
}
document.addEventListener('DOMContentLoaded', () => {
    void initPopup();
});
//# sourceMappingURL=popup.js.map
