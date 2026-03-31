/**
 * Popup script — displays convergence score and signals.
 */
import { initAuthSync } from '../background/auth-sync.js';
import { getAgents } from '../background/gateway-client.js';
const SIGNAL_NAMES = [
    'Session Duration',
    'Inter-Session Gap',
    'Response Latency',
    'Vocabulary Convergence',
    'Goal Boundary Erosion',
    'Initiative Balance',
    'Disengagement Resistance',
];
function ensureSignalRows() {
    const container = document.getElementById('signalList');
    if (!container || container.childElementCount > 0) {
        return;
    }
    container.innerHTML = SIGNAL_NAMES.map((name, index) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="signal-value-${index}">0.00</span>
      </div>
    `).join('');
}
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
async function loadCurrentPlatform() {
    const platformEl = document.getElementById('platform');
    if (!platformEl)
        return;
    try {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        const url = typeof tab?.url === 'string' ? new URL(tab.url) : null;
        platformEl.textContent = url?.hostname ?? 'Unknown';
    }
    catch {
        platformEl.textContent = 'Unknown';
    }
}
function updateUI(data) {
    const scoreEl = document.getElementById('scoreValue');
    const levelEl = document.getElementById('levelBadge');
    if (scoreEl)
        scoreEl.textContent = data.score.toFixed(2);
    if (levelEl) {
        levelEl.textContent = `Level ${data.level}`;
        levelEl.className = `level-badge level-${data.level}`;
    }
    data.signals.forEach((val, i) => {
        const el = document.getElementById(`signal-value-${i}`);
        if (el)
            el.textContent = val.toFixed(2);
    });
    const alertEl = document.getElementById('alertBanner');
    if (alertEl) {
        if (data.level >= 3) {
            alertEl.className = 'alert-banner active alert-danger';
            alertEl.textContent = `Convergence level ${data.level} detected. Consider taking a break.`;
        }
        else if (data.level >= 2) {
            alertEl.className = 'alert-banner active alert-warning';
            alertEl.textContent = 'Convergence is rising. Slow down and confirm intent before continuing.';
        }
        else {
            alertEl.className = 'alert-banner';
            alertEl.textContent = '';
        }
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
function updateSessionTimer() {
    const elapsed = Math.floor((Date.now() - sessionStart) / 60000);
    const timerEl = document.getElementById('sessionDuration');
    if (timerEl)
        timerEl.textContent = `${elapsed}m`;
}
updateSessionTimer();
setInterval(updateSessionTimer, 60000);
// Phase 4: Check auth state and update connection indicator, agent list, sync status
(async () => {
    ensureSignalRows();
    const auth = await initAuthSync();
    updateConnectionIndicator(auth.authenticated);
    if (auth.authenticated) {
        await loadAgentList();
    }
    else {
        const container = document.getElementById('agentList');
        if (container) {
            container.innerHTML = '<span class="agent-list-empty">Not connected to gateway</span>';
        }
    }
    await loadSyncStatus();
    await loadCurrentPlatform();
})();
//# sourceMappingURL=popup.js.map
