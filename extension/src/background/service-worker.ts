/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { cleanupSyncedEvents, initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();
const SCORE_REFRESH_ALARM = 'ghost-refresh-score';
const SYNC_CLEANUP_ALARM = 'ghost-cleanup-synced-events';

void initAuthSync().catch(() => {});
initAutoSync();

emitter.onScoreChange((score) => {
  void chrome.runtime.sendMessage({ type: 'score_update', score }).catch(() => {});
});

function scheduleBackgroundWork(): void {
  if (!chrome.alarms) {
    setInterval(() => {
      emitter.refreshScore();
    }, 30_000);
    setInterval(() => {
      void cleanupSyncedEvents().catch(() => {});
    }, 60 * 60 * 1000);
    return;
  }

  chrome.alarms.create(SCORE_REFRESH_ALARM, { periodInMinutes: 0.5 });
  chrome.alarms.create(SYNC_CLEANUP_ALARM, { periodInMinutes: 60 });
}

scheduleBackgroundWork();

chrome.runtime.onInstalled.addListener(() => {
  void chrome.storage.local.set({
    privacyLevel: 'standard',
    enabled: true,
  });
  scheduleBackgroundWork();
});

chrome.runtime.onStartup?.addListener(() => {
  scheduleBackgroundWork();
});

chrome.alarms?.onAlarm.addListener((alarm) => {
  if (alarm.name === SCORE_REFRESH_ALARM) {
    emitter.refreshScore();
    return;
  }

  if (alarm.name === SYNC_CLEANUP_ALARM) {
    void cleanupSyncedEvents().catch(() => {});
  }
});

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

  return false;
});

console.log('[GHOST] Background service worker initialized');
