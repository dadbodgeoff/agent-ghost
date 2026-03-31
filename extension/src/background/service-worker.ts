/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();
const latestStatus: {
  platform: string | null;
  sessionId: string | null;
  pageUrl: string | null;
  updatedAt: string | null;
} = {
  platform: null,
  sessionId: null,
  pageUrl: null,
  updatedAt: null,
};

void initAuthSync();
initAutoSync();

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'NEW_MESSAGE') {
    const timestamp = new Date().toISOString();
    latestStatus.platform = message.platform ?? latestStatus.platform;
    latestStatus.sessionId = message.sessionId ?? latestStatus.sessionId;
    latestStatus.pageUrl = message.pageUrl ?? latestStatus.pageUrl;
    latestStatus.updatedAt = timestamp;
    emitter.emit({
      eventType: 'InteractionMessage',
      platform: message.platform,
      role: message.role,
      contentHash: message.contentHash,
      timestamp,
      sessionId: message.sessionId,
      pageUrl: message.pageUrl,
    });
    sendResponse({ ok: true });
    return false;
  }

  if (message.type === 'SESSION_START') {
    const timestamp = new Date().toISOString();
    latestStatus.platform = message.platform ?? latestStatus.platform;
    latestStatus.sessionId = message.sessionId ?? latestStatus.sessionId;
    latestStatus.pageUrl = message.pageUrl ?? latestStatus.pageUrl;
    latestStatus.updatedAt = timestamp;
    emitter.emit({
      eventType: 'SessionStart',
      platform: message.platform,
      timestamp,
      sessionId: message.sessionId,
      pageUrl: message.pageUrl,
    });
    sendResponse({ ok: true });
    return false;
  }

  if (message.type === 'GET_SCORE') {
    sendResponse({
      score: emitter.getLatestScore(),
      platform: latestStatus.platform,
      sessionId: latestStatus.sessionId,
      pageUrl: latestStatus.pageUrl,
      updatedAt: latestStatus.updatedAt,
    });
    return false;
  }

  return false;
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

console.log('[GHOST] Background service worker initialized');
