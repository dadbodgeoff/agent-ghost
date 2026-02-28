/**
 * GHOST Convergence Monitor — Background Service Worker
 *
 * Coordinates between content scripts, native messaging host, popup,
 * and IndexedDB storage. Runs as a Chrome MV3 service worker or
 * Firefox background script.
 */

import { ITPEmitter } from "./itp-emitter.js";

const NATIVE_HOST = "ghost_convergence_monitor";
let nativePort = null;
let emitter = null;

// --- Lifecycle ---

chrome.runtime.onInstalled.addListener(() => {
  console.log("[GHOST] Extension installed");
  chrome.storage.local.set({ privacyLevel: "standard", enabled: true });
});

// --- Native Messaging ---

function connectNative() {
  try {
    nativePort = chrome.runtime.connectNative(NATIVE_HOST);
    nativePort.onMessage.addListener(handleNativeMessage);
    nativePort.onDisconnect.addListener(() => {
      console.warn("[GHOST] Native host disconnected:", chrome.runtime.lastError?.message);
      nativePort = null;
      // Fallback to IndexedDB storage
    });
    console.log("[GHOST] Connected to native messaging host");
  } catch (err) {
    console.warn("[GHOST] Native messaging unavailable:", err.message);
    nativePort = null;
  }
}

function handleNativeMessage(msg) {
  // Responses from convergence monitor (scores, interventions, etc.)
  if (msg.type === "score_update") {
    chrome.storage.local.set({ latestScore: msg.data });
    // Forward to popup if open
    chrome.runtime.sendMessage({ type: "score_update", data: msg.data }).catch(() => {});
  }
}

// --- Content Script Communication ---

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  switch (msg.type) {
    case "itp_event":
      handleITPEvent(msg.event);
      sendResponse({ ok: true });
      break;

    case "get_status":
      chrome.storage.local.get(["latestScore", "enabled", "privacyLevel"], (data) => {
        sendResponse({
          connected: nativePort !== null,
          enabled: data.enabled ?? true,
          privacyLevel: data.privacyLevel ?? "standard",
          latestScore: data.latestScore ?? null,
        });
      });
      return true; // async response

    case "set_privacy_level":
      chrome.storage.local.set({ privacyLevel: msg.level });
      sendResponse({ ok: true });
      break;

    case "set_enabled":
      chrome.storage.local.set({ enabled: msg.enabled });
      sendResponse({ ok: true });
      break;

    default:
      sendResponse({ error: `Unknown message type: ${msg.type}` });
  }
});

// --- ITP Event Handling ---

async function handleITPEvent(event) {
  if (!emitter) {
    emitter = new ITPEmitter();
  }

  // Apply privacy level
  const { privacyLevel } = await chrome.storage.local.get("privacyLevel");
  const processed = emitter.applyPrivacy(event, privacyLevel || "standard");

  // Send to native host if connected, otherwise store in IndexedDB
  if (nativePort) {
    try {
      nativePort.postMessage({ type: "itp_event", event: processed });
    } catch {
      await emitter.storeLocally(processed);
    }
  } else {
    await emitter.storeLocally(processed);
  }
}

// --- Initialization ---

connectNative();
