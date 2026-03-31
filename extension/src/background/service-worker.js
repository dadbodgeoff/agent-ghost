/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync.js';
import { ITPEmitter } from './itp-emitter.js';

const emitter = new ITPEmitter();
let refreshTimer = null;

function scheduleScoreRefresh() {
  if (refreshTimer) {
    clearInterval(refreshTimer);
  }
  refreshTimer = setInterval(() => {
    emitter.refreshScore();
  }, 30000);
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
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

  if (message.type === 'get_status') {
    sendResponse({
      connected: true,
      latestScore: {
        composite_score: emitter.getLatestScore(),
        level:
          emitter.getLatestScore() > 0.85
            ? 4
            : emitter.getLatestScore() > 0.7
              ? 3
              : emitter.getLatestScore() > 0.5
                ? 2
                : emitter.getLatestScore() > 0.3
                  ? 1
                  : 0,
        signals: [0, 0, 0, 0, 0, 0, 0],
      },
    });
  }

  return true;
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
