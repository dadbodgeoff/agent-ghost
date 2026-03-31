/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';

const emitter = new ITPEmitter();

void initAuthSync().catch(() => {
  // Keep the worker alive even when persisted auth state is unavailable.
});

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  switch (message.type) {
    case 'NEW_MESSAGE':
      emitter.emit({
        eventType: 'InteractionMessage',
        platform: message.platform,
        role: message.role,
        contentHash: message.contentHash,
        timestamp: new Date().toISOString(),
        sessionId: message.sessionId,
      });
      sendResponse({ ok: true });
      break;
    case 'SESSION_START':
      emitter.emit({
        eventType: 'SessionStart',
        platform: message.platform,
        timestamp: new Date().toISOString(),
        sessionId: message.sessionId,
      });
      sendResponse({ ok: true });
      break;
    case 'GET_SCORE':
      sendResponse({ score: emitter.getLatestScore() });
      break;
    default:
      sendResponse({ ok: false });
  }

  return true; // Keep channel open for async response
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

console.log('[GHOST] Background service worker initialized');
