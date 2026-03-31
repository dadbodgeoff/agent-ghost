/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { ITPEmitter } from './itp-emitter';

const emitter = new ITPEmitter();

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (!message || typeof message.type !== 'string') {
    return false;
  }

  try {
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
        return false;
      case 'SESSION_START':
        emitter.emit({
          eventType: 'SessionStart',
          platform: message.platform,
          timestamp: new Date().toISOString(),
          sessionId: message.sessionId,
        });
        sendResponse({ ok: true });
        return false;
      case 'GET_SCORE':
        sendResponse({ score: emitter.getLatestScore() });
        return false;
      default:
        return false;
    }
  } catch (error) {
    console.warn('[GHOST] Failed to handle background message:', error);
    sendResponse({ ok: false });
    return false;
  }
});

// Periodic score refresh
setInterval(() => {
  try {
    emitter.refreshScore();
  } catch (error) {
    console.warn('[GHOST] Score refresh failed:', error);
  }
}, 30_000);

console.log('[GHOST] Background service worker initialized');
