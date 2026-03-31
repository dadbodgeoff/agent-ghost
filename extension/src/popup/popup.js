/**
 * Legacy source popup script kept aligned with popup.ts.
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

const LEVEL_LABELS = ['Level 0', 'Level 1', 'Level 2', 'Level 3', 'Level 4'];
const signalIds = SIGNAL_NAMES.map((_, index) => ({
  value: `signal-value-${index}`,
  bar: `signal-bar-${index}`,
}));
const sessionStartTime = Date.now();

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
  if (!container) return;
  container.innerHTML = SIGNAL_NAMES.map(
    (name, index) => `
      <div class="signal-row">
        <span class="signal-name">${name}</span>
        <span class="signal-value" id="${signalIds[index].value}">0.000</span>
        <div class="signal-bar">
          <div class="signal-bar-fill" id="${signalIds[index].bar}" style="width: 0%"></div>
        </div>
      </div>
    `,
  ).join('');
}

function updateSessionTimer() {
  const timerEl = document.getElementById('sessionDuration');
  if (!timerEl) return;
  const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
  const hours = Math.floor(elapsed / 3600);
  const minutes = Math.floor((elapsed % 3600) / 60);
  const seconds = elapsed % 60;
  timerEl.textContent = `${hours}h ${minutes}m ${seconds}s`;
}

function scoreColor(score) {
  if (score < 0.3) return '#22c55e';
  if (score < 0.5) return '#eab308';
  if (score < 0.7) return '#f97316';
  return '#ef4444';
}

function updateAlert(level) {
  const alertEl = document.getElementById('alertBanner');
  if (!alertEl) return;
  if (level >= 3) {
    alertEl.className = 'alert-banner active alert-danger';
    alertEl.textContent =
      level >= 4
        ? 'Intervention Level 4. External escalation active.'
        : 'Intervention Level 3. Consider ending the session.';
    return;
  }
  if (level >= 2) {
    alertEl.className = 'alert-banner active alert-warning';
    alertEl.textContent = 'Intervention Level 2. Acknowledge before continuing.';
    return;
  }
  alertEl.className = 'alert-banner';
  alertEl.textContent = '';
}

function updateUI(data) {
  const scoreEl = document.getElementById('scoreValue');
  const levelEl = document.getElementById('levelBadge');
  const platformEl = document.getElementById('platform');
  if (scoreEl) {
    scoreEl.textContent = data.score.toFixed(2);
    scoreEl.style.color = scoreColor(data.score);
  }
  if (levelEl) {
    levelEl.textContent = LEVEL_LABELS[data.level] || `Level ${data.level}`;
    levelEl.className = `level-badge level-${Math.min(Math.max(data.level, 0), 4)}`;
  }
  if (platformEl) {
    platformEl.textContent = data.platform || 'Background worker';
  }
  data.signals.forEach((value, index) => {
    const ids = signalIds[index];
    if (!ids) return;
    const valueEl = document.getElementById(ids.value);
    const barEl = document.getElementById(ids.bar);
    if (valueEl) valueEl.textContent = value.toFixed(3);
    if (barEl) {
      const clamped = Math.max(0, Math.min(value, 1));
      barEl.style.width = `${(clamped * 100).toFixed(0)}%`;
      barEl.style.background = scoreColor(clamped);
    }
  });
  updateAlert(data.level);
}

function requestScore() {
  chrome.runtime.sendMessage({ type: 'GET_SCORE' }, (response) => {
    if (chrome.runtime.lastError || !response || typeof response.score !== 'number') return;
    const score = response.score;
    const level = score > 0.85 ? 4 : score > 0.7 ? 3 : score > 0.5 ? 2 : score > 0.3 ? 1 : 0;
    updateUI({
      score,
      level,
      signals: Array.from({ length: SIGNAL_NAMES.length }, () => 0),
    });
  });
}

document.addEventListener('DOMContentLoaded', () => {
  renderSignalList();
  updateSessionTimer();
  setInterval(updateSessionTimer, 1000);
  updateConnectionIndicator(false);
  requestScore();
});
