/**
 * GHOST Convergence Monitor — Content Script Observer
 *
 * Injected into AI chat platform pages. Detects new messages via
 * MutationObserver, extracts content, and sends ITP events to the
 * background service worker.
 *
 * Platform detection is URL-based. Each platform has a specific adapter
 * that knows how to find message containers and parse message elements.
 */

import { ChatGPTAdapter } from "./adapters/chatgpt.js";
import { ClaudeAdapter } from "./adapters/claude.js";
import { CharacterAIAdapter } from "./adapters/character-ai.js";
import { GeminiAdapter } from "./adapters/gemini.js";
import { DeepSeekAdapter } from "./adapters/deepseek.js";
import { GrokAdapter } from "./adapters/grok.js";

const ADAPTERS = [
  ChatGPTAdapter,
  ClaudeAdapter,
  CharacterAIAdapter,
  GeminiAdapter,
  DeepSeekAdapter,
  GrokAdapter,
];

let activeAdapter = null;
let observer = null;
let sessionId = null;
let messageCount = 0;

function detectPlatform() {
  const url = window.location.href;
  for (const Adapter of ADAPTERS) {
    if (Adapter.matches(url)) {
      return new Adapter();
    }
  }
  return null;
}

function generateSessionId() {
  return crypto.randomUUID();
}

function sendEvent(event) {
  chrome.runtime.sendMessage({ type: "itp_event", event }).catch(() => {
    // Extension context invalidated — page was navigated away
  });
}

function emitSessionStart() {
  sessionId = generateSessionId();
  messageCount = 0;
  sendEvent({
    event_type: "SessionStart",
    session_id: sessionId,
    platform: activeAdapter.platformName,
    timestamp: new Date().toISOString(),
    source: "browser_extension",
  });
}

function emitMessage(role, content) {
  messageCount++;
  sendEvent({
    event_type: "InteractionMessage",
    session_id: sessionId,
    message_id: `${sessionId}-${messageCount}`,
    role,
    content,
    timestamp: new Date().toISOString(),
    source: "browser_extension",
    platform: activeAdapter.platformName,
  });
}

function startObserving() {
  activeAdapter = detectPlatform();
  if (!activeAdapter) {
    console.log("[GHOST] No matching platform adapter for", window.location.href);
    return;
  }

  console.log("[GHOST] Platform detected:", activeAdapter.platformName);
  emitSessionStart();

  // Wait for the message container to appear
  const waitForContainer = setInterval(() => {
    const container = activeAdapter.getMessageContainer();
    if (container) {
      clearInterval(waitForContainer);
      setupObserver(container);
    }
  }, 1000);

  // Give up after 30 seconds
  setTimeout(() => clearInterval(waitForContainer), 30000);
}

function setupObserver(container) {
  // Process existing messages
  const existing = activeAdapter.getExistingMessages(container);
  for (const msg of existing) {
    const parsed = activeAdapter.parseMessage(msg);
    if (parsed) {
      emitMessage(parsed.role, parsed.content);
    }
  }

  // Watch for new messages
  observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (node.nodeType !== Node.ELEMENT_NODE) continue;
        const parsed = activeAdapter.parseMessage(node);
        if (parsed) {
          emitMessage(parsed.role, parsed.content);
        }
      }
    }
  });

  observer.observe(container, { childList: true, subtree: true });
  console.log("[GHOST] Observer active on", activeAdapter.platformName);
}

// Handle page navigation (SPA)
let lastUrl = window.location.href;
const urlObserver = new MutationObserver(() => {
  if (window.location.href !== lastUrl) {
    lastUrl = window.location.href;
    if (observer) {
      observer.disconnect();
      observer = null;
    }
    // Emit session end for previous session
    if (sessionId) {
      sendEvent({
        event_type: "SessionEnd",
        session_id: sessionId,
        timestamp: new Date().toISOString(),
        message_count: messageCount,
        source: "browser_extension",
      });
    }
    // Start new session for new URL
    startObserving();
  }
});
urlObserver.observe(document.body, { childList: true, subtree: true });

// Page unload — emit session end
window.addEventListener("beforeunload", () => {
  if (sessionId) {
    sendEvent({
      event_type: "SessionEnd",
      session_id: sessionId,
      timestamp: new Date().toISOString(),
      message_count: messageCount,
      source: "browser_extension",
    });
  }
});

// Start
startObserving();
