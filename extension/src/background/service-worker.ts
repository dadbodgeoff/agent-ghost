/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();

async function bootstrap(): Promise<void> {
  try {
    await initAuthSync();
  } catch (error) {
    console.warn('[GHOST] Failed to restore auth state', error);
  }

  try {
    initAutoSync();
  } catch (error) {
    console.warn('[GHOST] Failed to start auto-sync', error);
  }
}

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'NEW_MESSAGE') {
    try {
      emitter.emit({
        eventType: 'InteractionMessage',
        platform: message.platform,
        role: message.role,
        contentHash: message.contentHash,
        timestamp: new Date().toISOString(),
        sessionId: message.sessionId,
      });
      sendResponse({ ok: true });
    } catch (error) {
      sendResponse({ ok: false, error: error instanceof Error ? error.message : 'Unknown error' });
    }
  }

  if (message.type === 'SESSION_START') {
    try {
      emitter.emit({
        eventType: 'SessionStart',
        platform: message.platform,
        timestamp: new Date().toISOString(),
        sessionId: message.sessionId,
      });
      sendResponse({ ok: true });
    } catch (error) {
      sendResponse({ ok: false, error: error instanceof Error ? error.message : 'Unknown error' });
    }
  }

  if (message.type === 'GET_SCORE') {
    sendResponse({ score: emitter.getLatestScore() });
  }

  return true; // Keep channel open for async response
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

void bootstrap();

console.log('[GHOST] Background service worker initialized');
