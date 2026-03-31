/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { cleanupSyncedEvents, initAutoSync, syncPendingEvents } from '../storage/sync';

const emitter = new ITPEmitter();

async function flushPendingEvents(): Promise<void> {
  const result = await syncPendingEvents();
  if (result.synced > 0) {
    await chrome.storage.local.set({ 'ghost-last-sync': Date.now() });
  }
}

async function bootstrap(): Promise<void> {
  await initAuthSync();
  initAutoSync();
  await flushPendingEvents();
  await cleanupSyncedEvents();
}

void bootstrap();

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  void sender;

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

  return false;
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

setInterval(() => {
  void flushPendingEvents();
}, 60_000);

setInterval(() => {
  void cleanupSyncedEvents();
}, 60 * 60 * 1000);

console.log('[GHOST] Background service worker initialized');
