/**
 * GHOST Convergence Monitor — Background Service Worker
 *
 * Coordinates between content scripts, native messaging host, popup,
 * and IndexedDB storage. Runs as a Chrome MV3 service worker or
 * Firefox background script.
 */

import { ITPEmitter } from "./itp-emitter.js";

const NATIVE_HOST = "ghost_convergence_monitor";
const SCORE_REFRESH_ALARM = "ghost-score-refresh";
const SCORE_REFRESH_PERIOD_MINUTES = 0.5;

let nativePort = null;
let emitter = null;
let refreshIntervalHandle = null;

function ensureEmitter() {
  if (!emitter) {
    emitter = new ITPEmitter();
  }
  return emitter;
}

async function initBackgroundState() {
  await chrome.storage.local.set({ privacyLevel: "standard", enabled: true });
  connectNative();
  scheduleScoreRefresh();
}

chrome.runtime.onInstalled.addListener(() => {
  console.log("[GHOST] Extension installed");
  void initBackgroundState();
});

chrome.runtime.onStartup?.addListener(() => {
  void initBackgroundState();
});

function connectNative() {
  try {
    nativePort = chrome.runtime.connectNative(NATIVE_HOST);
    nativePort.onMessage.addListener(handleNativeMessage);
    nativePort.onDisconnect.addListener(() => {
      console.warn("[GHOST] Native host disconnected:", chrome.runtime.lastError?.message);
      nativePort = null;
    });
    console.log("[GHOST] Connected to native messaging host");
  } catch (err) {
    console.warn("[GHOST] Native messaging unavailable:", err?.message ?? String(err));
    nativePort = null;
  }
}

function handleNativeMessage(msg) {
  if (msg.type === "score_update") {
    void chrome.storage.local.set({ latestScore: msg.data });
    try {
      chrome.runtime.sendMessage({ type: "score_update", data: msg.data });
    } catch {
      // Popup not open.
    }
  }
}

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  switch (msg.type) {
    case "itp_event":
      void handleITPEvent(msg.event).then(() => sendResponse({ ok: true })).catch((error) => {
        sendResponse({ error: error instanceof Error ? error.message : "Failed to handle ITP event" });
      });
      return true;

    case "get_status":
      chrome.storage.local.get(["latestScore", "enabled", "privacyLevel"], (data) => {
        sendResponse({
          connected: nativePort !== null,
          enabled: data.enabled ?? true,
          privacyLevel: data.privacyLevel ?? "standard",
          latestScore: data.latestScore ?? null,
        });
      });
      return true;

    case "set_privacy_level":
      void chrome.storage.local.set({ privacyLevel: msg.level }).then(() => sendResponse({ ok: true }));
      return true;

    case "set_enabled":
      void chrome.storage.local.set({ enabled: msg.enabled }).then(() => sendResponse({ ok: true }));
      return true;

    default:
      sendResponse({ error: `Unknown message type: ${msg.type}` });
      return false;
  }
});

async function handleITPEvent(event) {
  const activeEmitter = ensureEmitter();
  const { privacyLevel } = await chrome.storage.local.get("privacyLevel");
  const processed = activeEmitter.applyPrivacy(event, privacyLevel || "standard");

  if (nativePort) {
    try {
      nativePort.postMessage({ type: "itp_event", event: processed });
      return;
    } catch {
      nativePort = null;
    }
  }

  await activeEmitter.storeLocally(processed);
}

function refreshScore() {
  ensureEmitter().refreshScore();
}

function scheduleScoreRefresh() {
  if (chrome.alarms?.create) {
    chrome.alarms.create(SCORE_REFRESH_ALARM, { periodInMinutes: SCORE_REFRESH_PERIOD_MINUTES });
    return;
  }

  if (refreshIntervalHandle == null) {
    refreshIntervalHandle = setInterval(refreshScore, 30_000);
  }
}

chrome.alarms?.onAlarm.addListener((alarm) => {
  if (alarm.name === SCORE_REFRESH_ALARM) {
    refreshScore();
  }
});

void initBackgroundState();
