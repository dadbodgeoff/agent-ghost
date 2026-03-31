/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';

const emitter = new ITPEmitter();
let refreshTimer: ReturnType<typeof setInterval> | null = null;

function scheduleScoreRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer);
  }
  refreshTimer = setInterval(() => {
    emitter.refreshScore();
  }, 30_000);
}

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'NEW_MESSAGE') {
    emitter.emit({
      eventType: 'InteractionMessage',
      platform: message.platform,
      role: message.role,
      contentHash: message.contentHash,
      timestamp: new Date().toISOString(),
      sessionId: message.sessionId,
    });
    sendResponse({ ok: true });
  }

  if (message.type === 'SESSION_START') {
    emitter.emit({
      eventType: 'SessionStart',
      platform: message.platform,
      timestamp: new Date().toISOString(),
      sessionId: message.sessionId,
    });
    sendResponse({ ok: true });
  }

  if (message.type === 'GET_SCORE') {
    sendResponse({ score: emitter.getLatestScore() });
  }

  return true; // Keep channel open for async response
});

chrome.runtime.onStartup.addListener(() => {
  void initAuthSync();
});

chrome.runtime.onInstalled.addListener(() => {
  void initAuthSync();
});

void initAuthSync();
scheduleScoreRefresh();

console.log('[GHOST] Background service worker initialized');
