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

function scoreColor(score) {
  if (score < 0.3) return "#22c55e";
  if (score < 0.5) return "#eab308";
  if (score < 0.7) return "#f97316";
  return "#ef4444";
}

function setText(id, value) {
  const element = document.getElementById(id);
  if (element) {
    element.textContent = value;
  }
  return element;
}

function renderStatusMessage(message) {
  const container = document.getElementById("agentList");
  if (!container) return;
  container.replaceChildren();
  const empty = document.createElement("span");
  empty.className = "agent-list-empty";
  empty.textContent = message;
  container.append(empty);
}

function renderAgentList(agents) {
  const container = document.getElementById("agentList");
  if (!container) return;

  container.replaceChildren();
  for (const agent of agents) {
    const row = document.createElement("div");
    row.className = "agent-list-item";

    const name = document.createElement("span");
    name.className = "agent-name";
    name.textContent = agent.name || agent.id || "Unknown agent";

    const state = document.createElement("span");
    state.className = "agent-state";
    state.textContent = agent.state || "unknown";

    row.append(name, state);
    container.append(row);
  }
}

function renderSignalList() {
  const container = document.getElementById("signalList");
  if (!container) return;

  container.replaceChildren();
  SIGNAL_NAMES.forEach((name, i) => {
    const row = document.createElement("div");
    row.className = "signal-row";

    const label = document.createElement("span");
    label.className = "signal-name";
    label.textContent = name;

    const value = document.createElement("span");
    value.className = "signal-value";
    value.id = `signal-value-${i}`;
    value.textContent = "0.000";

    const bar = document.createElement("div");
    bar.className = "signal-bar";

    const fill = document.createElement("div");
    fill.className = "signal-bar-fill";
    fill.id = `signal-bar-${i}`;
    fill.style.width = "0%";

    bar.append(fill);
    row.append(label, value, bar);
    container.append(row);
  });
}

async function requestStatus() {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: "get_status" }, (response) => {
      if (chrome.runtime.lastError || !response) {
        resolve(null);
        return;
      }

      const dot = document.getElementById("statusDot");
      const label = document.getElementById("statusLabel");
      if (dot) {
        dot.className = `status-dot ${response.connected ? "connected" : "disconnected"}`;
        dot.setAttribute("aria-label", response.connected ? "Connected" : "Disconnected");
      }
      if (label) {
        label.className = `status-label ${response.connected ? "connected" : "disconnected"}`;
        label.textContent = response.connected ? "Connected" : "Disconnected";
      }

      if (response.latestScore) {
        updateDisplay(response.latestScore);
      }

      resolve(response);
    });
  });
}

function updateDisplay(data) {
  const score = typeof data?.composite_score === "number" ? data.composite_score : 0;
  const level = typeof data?.level === "number" ? data.level : 0;

  const scoreEl = setText("scoreValue", score.toFixed(2));
  if (scoreEl) {
    scoreEl.style.color = scoreColor(score);
  }

  const badge = setText("levelBadge", LEVEL_LABELS[level] || `Level ${level}`);
  if (badge) {
    badge.className = `level-badge ${LEVEL_CLASSES[level] || "level-0"}`;
  }

  if (Array.isArray(data?.signals)) {
    data.signals.forEach((value, i) => {
      if (typeof value !== "number") return;
      setText(`signal-value-${i}`, value.toFixed(3));
      const barEl = document.getElementById(`signal-bar-${i}`);
      if (barEl) {
        barEl.style.width = `${Math.max(0, Math.min(100, value * 100)).toFixed(0)}%`;
        barEl.style.background = scoreColor(value);
      }
    });
  }

  if (typeof data?.platform === "string" && data.platform) {
    setText("platform", data.platform);
  }

  const banner = document.getElementById("alertBanner");
  if (!banner) return;

  if (level >= 3) {
    banner.className = "alert-banner active alert-danger";
    banner.textContent = `Intervention Level ${level} — ${level === 4 ? "External escalation active" : "Session may be terminated"}`;
  } else if (level >= 2) {
    banner.className = "alert-banner active alert-warning";
    banner.textContent = "Intervention Level 2 — Acknowledgment required";
  } else {
    banner.className = "alert-banner";
    banner.textContent = "";
  }
}

function startSessionTimer() {
  sessionStartTime = Date.now();
  const el = document.getElementById("sessionDuration");
  if (!el) return;

  const update = () => {
    const elapsed = Math.floor((Date.now() - sessionStartTime) / 1000);
    const h = Math.floor(elapsed / 3600);
    const m = Math.floor((elapsed % 3600) / 60);
    const s = elapsed % 60;
    el.textContent = `${h}h ${m}m ${s}s`;
  };

  update();
  setInterval(update, 1000);
}

async function loadAgents(status) {
  if (!status?.connected) {
    renderStatusMessage("Not connected to gateway");
    return;
  }

  renderStatusMessage("Agents are only available from the native gateway surface.");
}

async function loadSyncStatus() {
  const stored = await chrome.storage.local.get("ghost-last-sync");
  const ts = stored["ghost-last-sync"];
  setText("syncStatus", typeof ts === "number" ? new Date(ts).toLocaleTimeString() : "never");
}

document.addEventListener("DOMContentLoaded", async () => {
  renderSignalList();
  startSessionTimer();

  chrome.runtime.onMessage.addListener((msg) => {
    if (msg.type === "score_update") {
      updateDisplay(msg.data);
    }
  });

  const status = await requestStatus();
  await loadAgents(status);
  await loadSyncStatus();

  setInterval(async () => {
    const latestStatus = await requestStatus();
    await loadAgents(latestStatus);
    await loadSyncStatus();
  }, 5000);
});
