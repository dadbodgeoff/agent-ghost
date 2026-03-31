/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync.js';
import { ITPEmitter } from './itp-emitter.js';
import { initAutoSync } from '../storage/sync.js';

const emitter = new ITPEmitter();

void initAuthSync();
initAutoSync();

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  const type = typeof message?.type === 'string' ? message.type : '';

  if (type === 'NEW_MESSAGE') {
    void emitter
      .emit({
        eventType: 'InteractionMessage',
        platform: typeof message.platform === 'string' ? message.platform : 'unknown',
        role: typeof message.role === 'string' ? message.role : undefined,
        contentHash: typeof message.contentHash === 'string' ? message.contentHash : undefined,
        timestamp: new Date().toISOString(),
        sessionId: typeof message.sessionId === 'string' ? message.sessionId : undefined,
      })
      .then(() => sendResponse({ ok: true }))
      .catch((error: unknown) =>
        sendResponse({
          ok: false,
          error: error instanceof Error ? error.message : 'Failed to store message event',
        }),
      );
    return true;
  }

  if (type === 'SESSION_START') {
    void emitter
      .emit({
        eventType: 'SessionStart',
        platform: typeof message.platform === 'string' ? message.platform : 'unknown',
        timestamp: new Date().toISOString(),
        sessionId: typeof message.sessionId === 'string' ? message.sessionId : undefined,
      })
      .then(() => sendResponse({ ok: true }))
      .catch((error: unknown) =>
        sendResponse({
          ok: false,
          error: error instanceof Error ? error.message : 'Failed to store session event',
        }),
      );
    return true;
  }

  if (type === 'GET_SCORE') {
    sendResponse({ score: emitter.getLatestScore() });
    return false;
  }

  return false;
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

console.log('[GHOST] Background service worker initialized');
