/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { ITPEmitter } from './itp-emitter';

const emitter = new ITPEmitter();
const SCORE_REFRESH_ALARM = 'ghost-refresh-score';
const SCORE_REFRESH_PERIOD_MINUTES = 0.5;

function refreshScore(): void {
  emitter.refreshScore();
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

  return false;
});

chrome.runtime.onInstalled.addListener(() => {
  chrome.alarms.create(SCORE_REFRESH_ALARM, {
    periodInMinutes: SCORE_REFRESH_PERIOD_MINUTES,
  });
  refreshScore();
});

chrome.runtime.onStartup.addListener(() => {
  chrome.alarms.create(SCORE_REFRESH_ALARM, {
    periodInMinutes: SCORE_REFRESH_PERIOD_MINUTES,
  });
  refreshScore();
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === SCORE_REFRESH_ALARM) {
    refreshScore();
  }
});

refreshScore();

console.log('[GHOST] Background service worker initialized');
