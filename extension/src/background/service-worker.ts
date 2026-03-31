/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();
const SCORE_REFRESH_ALARM = 'ghost-refresh-score';

void initAuthSync().catch(() => {});
initAutoSync();

function scheduleScoreRefresh(): void {
  chrome.alarms.create(SCORE_REFRESH_ALARM, { periodInMinutes: 0.5 });
}

scheduleScoreRefresh();
chrome.runtime.onInstalled.addListener(() => scheduleScoreRefresh());
chrome.runtime.onStartup.addListener(() => scheduleScoreRefresh());
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === SCORE_REFRESH_ALARM) {
    emitter.refreshScore();
  }
});

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (!message || typeof message.type !== 'string') {
    sendResponse({ ok: false, error: 'Invalid message payload' });
    return false;
  }

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

  sendResponse({ ok: false, error: `Unsupported message type: ${message.type}` });
  return false;
});

console.log('[GHOST] Background service worker initialized');
