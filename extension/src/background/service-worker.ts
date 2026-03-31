/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { ITPEmitter } from './itp-emitter';

const emitter = new ITPEmitter();
const SCORE_REFRESH_ALARM = 'ghost-refresh-score';
const SCORE_REFRESH_PERIOD_MINUTES = 0.5;

function scheduleScoreRefresh() {
  if (!chrome.alarms?.create) return;

  chrome.alarms.create(SCORE_REFRESH_ALARM, {
    periodInMinutes: SCORE_REFRESH_PERIOD_MINUTES,
  });
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

chrome.runtime.onInstalled.addListener(() => {
  scheduleScoreRefresh();
});

if (chrome.runtime.onStartup) {
  chrome.runtime.onStartup.addListener(() => {
    scheduleScoreRefresh();
  });
}

if (chrome.alarms?.onAlarm) {
  chrome.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === SCORE_REFRESH_ALARM) {
      void emitter.refreshScore();
    }
  });
  scheduleScoreRefresh();
} else {
  // Firefox MV2 background scripts stay alive; use an interval there.
  setInterval(() => {
    void emitter.refreshScore();
  }, 30_000);
}

console.log('[GHOST] Background service worker initialized');
