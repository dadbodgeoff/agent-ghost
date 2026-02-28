/**
 * GHOST Convergence Monitor — Popup Script
 *
 * Renders the score gauge, signal list, session timer, and alert banner.
 * Communicates with the background service worker for live data.
 */

const SIGNAL_NAMES = [
  "Session Duration",
  "Inter-Session Gap",
  "Response Latency",
  "Vocabulary Convergence",
  "Goal Boundary Erosion",
  "Initiative Balance",
  "Disengagement Resistance",
];

const LEVEL_LABELS = ["Level 0 — Normal", "Level 1 — Soft", "Level 2 — Active", "Level 3 — Hard", "Level 4 — External"];
const LEVEL_CLASSES = ["level-0", "level-1", "level-2", "level-3", "level-4"];

let sessionStartTime = null;
let timerInterval = null;

// --- Initialization ---

document.addEventListener("DOMContentLoaded", () => {
  renderSignalList();
  requestStatus();
  startSessionTimer();

  // Listen for live updates from background
  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.type === "score_update") {
      updateDisplay(msg.data);
    }
  });

  // Poll for updates every 5 seconds
  setInterval(requestStatus, 5000);
});

function requestStatus() {
  chrome.runtime.sendMessage({ type: "get_status" }, (response) => {
    if (chrome.runtime.lastError) return;
    if (!response) return;

    const dot = document.getElementById("statusDot");
    dot.className = `status-dot ${response.connected ? "connected" : "disconnected"}`;
    dot.setAttribute("aria-label", response.connected ? "Connected" : "Disconnected");

    if (response.latestScore) {
      updateDisplay(response.latestScore);
    }
  });
}

function updateDisplay(data) {
  // Score
  const scoreEl = document.getElementById("scoreValue");
  const score = typeof data.composite_score === "number" ? data.composite_score : 0;
  scoreEl.textContent = score.toFixed(2);
  scoreEl.style.color = scoreColor(score);

  // Level
  const level = typeof data.level === "number" ? data.level : 0;
  const badge = document.getElementById("levelBadge");
  badge.textContent = LEVEL_LABELS[level] || `Level ${level}`;
  badge.className = `level-badge ${LEVEL_CLASSES[level] || "level-0"}`;

  // Signals
  if (Array.isArray(data.signals)) {
    data.signals.forEach((val, i) => {
      const valueEl = document.getElementById(`signal-value-${i}`);
      const barEl = document.getElementById(`signal-bar-${i}`);
      if (valueEl) valueEl.textContent = val.toFixed(3);
      if (barEl) {
        barEl.style.width = `${(val * 100).toFixed(0)}%`;
        barEl.style.background = scoreColor(val);
      }
    });
  }

  // Platform
  if (data.platform) {
    document.getElementById("platform").textContent = data.platform;
  }

  // Alert banner
  const banner = document.getElementById("alertBanner");
  if (level >= 3) {
    banner.className = "alert-banner active alert-danger";
    banner.textContent = `Intervention Level ${level} — ${level === 4 ? "External escalation active" : "Session may be terminated"}`;
  } else if (level >= 2) {
    banner.className = "alert-banner active alert-warning";
    banner.textContent = "Intervention Level 2 — Acknowledgment required";
  } else {
    banner.className = "alert-banner";
  }
}

function renderSignalList() {
  const container = document.getElementById("signalList");
  container.innerHTML = SIGNAL_NAMES.map((name, i) => `
    <div class="signal-row">
      <span class="signal-name">${name}</span>
      <span class="signal-value" id="signal-value-${i}">0.000</span>
      <div class="signal-bar">
        <div class="signal-bar-fill" id="signal-bar-${i}" style="width:0%"></div>
      </div>
    </div>
  `).join("");
}

function startSessionTimer() {
  sessionStartTime = Date.now();
  const el = document.getElementById("sessionDuration");
  timerInterval = setInterval(() => {
    const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
    const h = Math.floor(elapsed / 3600);
    const m = Math.floor((elapsed % 3600) / 60);
    const s = elapsed % 60;
    el.textContent = `${h}h ${m}m ${s}s`;
  }, 1000);
}

function scoreColor(score) {
  if (score < 0.3) return "#22c55e";
  if (score < 0.5) return "#eab308";
  if (score < 0.7) return "#f97316";
  return "#ef4444";
}
