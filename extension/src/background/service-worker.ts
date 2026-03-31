/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();
const SCORE_REFRESH_ALARM = 'ghost-refresh-score';
let intervalHandle: number | null = null;

async function refreshScore(): Promise<void> {
  emitter.refreshScore();
}

function scheduleScoreRefresh(): void {
  if (typeof chrome.alarms?.create === 'function') {
    chrome.alarms.create(SCORE_REFRESH_ALARM, { periodInMinutes: 0.5 });
    return;
  }

  if (intervalHandle === null) {
    intervalHandle = self.setInterval(() => {
      void refreshScore();
    }, 30_000);
  }
}

async function initializeBackground(): Promise<void> {
  await initAuthSync();
  initAutoSync();
  scheduleScoreRefresh();
  await refreshScore();
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

if (typeof chrome.alarms?.onAlarm?.addListener === 'function') {
  chrome.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === SCORE_REFRESH_ALARM) {
      void refreshScore();
    }
  });
}

chrome.runtime.onInstalled.addListener(() => {
  void initializeBackground();
});

chrome.runtime.onStartup?.addListener(() => {
  void initializeBackground();
});

void initializeBackground();

console.log('[GHOST] Background service worker initialized');
