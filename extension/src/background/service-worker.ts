/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { ensureAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();

void ensureAuthSync();
initAutoSync();

// Listen for messages from content scripts
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
    return false;
  }

  if (message.type === 'SESSION_START') {
    emitter.emit({
      eventType: 'SessionStart',
      platform: message.platform,
      timestamp: new Date().toISOString(),
      sessionId: message.sessionId,
    });
    sendResponse({ ok: true });
    return false;
  }

  if (message.type === 'GET_SCORE') {
    sendResponse({ score: emitter.getLatestScore() });
    return false;
  }

  sendResponse({ error: `Unknown message type: ${String(message?.type ?? 'unknown')}` });
  return false;
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

console.log('[GHOST] Background service worker initialized');
