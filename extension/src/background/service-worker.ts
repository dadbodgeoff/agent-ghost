/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { getAuthState, initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';
import { initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();

function levelFromScore(score: number): number {
  if (score > 0.85) return 4;
  if (score > 0.7) return 3;
  if (score > 0.5) return 2;
  if (score > 0.3) return 1;
  return 0;
}

function buildScoreSnapshot() {
  const score = emitter.getLatestScore();
  return {
    composite_score: score,
    level: levelFromScore(score),
    signals: [0, 0, 0, 0, 0, 0, 0],
    platform: 'native-monitor',
  };
}

function broadcastScoreUpdate(): void {
  const payload = buildScoreSnapshot();
  chrome.runtime.sendMessage({ type: 'score_update', data: payload }).catch(() => {});
}

void initAuthSync();
initAutoSync();
emitter.onScoreUpdate(() => {
  broadcastScoreUpdate();
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
    sendResponse({ score: emitter.getLatestScore(), snapshot: buildScoreSnapshot() });
  }

  if (message.type === 'get_status') {
    const auth = getAuthState();
    sendResponse({
      connected: auth.authenticated,
      latestScore: buildScoreSnapshot(),
    });
  }

  return true; // Keep channel open for async response
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

emitter.refreshScore();

console.log('[GHOST] Background service worker initialized');
