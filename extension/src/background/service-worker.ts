/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync, getAuthState } from './auth-sync';
import { getAgents } from './gateway-client';
import { ITPEmitter } from './itp-emitter';
import { initAutoSync } from '../storage/sync';

const emitter = new ITPEmitter();

async function getStatus() {
  const auth = getAuthState();
  const latestScore = emitter.getLatestScore();
  return {
    authenticated: auth.authenticated,
    gatewayUrl: auth.gatewayUrl,
    lastValidated: auth.lastValidated,
    connected: auth.authenticated,
    latestScore: {
      composite_score: latestScore,
      level: latestScore > 0.85 ? 4 : latestScore > 0.7 ? 3 : latestScore > 0.5 ? 2 : latestScore > 0.3 ? 1 : 0,
      signals: [0, 0, 0, 0, 0, 0, 0],
    },
  };
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

  if (message.type === 'GET_STATUS') {
    void getStatus()
      .then((status) => sendResponse(status))
      .catch(() =>
        sendResponse({
          authenticated: false,
          gatewayUrl: '',
          lastValidated: 0,
          connected: false,
          latestScore: null,
        }),
      );
  }

  if (message.type === 'GET_AGENTS') {
    void getAgents()
      .then((agents) => sendResponse({ agents }))
      .catch((error) =>
        sendResponse({
          agents: [],
          error: error instanceof Error ? error.message : 'Unable to load agents',
        }),
      );
  }

  return true; // Keep channel open for async response
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

void initAuthSync().catch(() => {});
initAutoSync();

console.log('[GHOST] Background service worker initialized');
