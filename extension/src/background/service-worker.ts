/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAutoSync } from '../storage/sync';
import { getAuthState, initAuthSync } from './auth-sync';
import { getAgents } from './gateway-client';
import { ITPEmitter } from './itp-emitter';

const emitter = new ITPEmitter();
const bootstrap = (async () => {
  await initAuthSync();
  initAutoSync();
})();

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
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

  if (message.type === 'GET_AUTH_STATE') {
    void bootstrap.finally(() => {
      sendResponse({ auth: getAuthState() });
    });
    return true;
  }

  if (message.type === 'GET_AGENTS') {
    void bootstrap
      .then(async () => {
        const auth = getAuthState();
        if (!auth.authenticated) {
          sendResponse({ agents: [], error: 'Not connected to gateway' });
          return;
        }

        const agents = await getAgents();
        sendResponse({ agents });
      })
      .catch((error: unknown) => {
        sendResponse({
          agents: [],
          error: error instanceof Error ? error.message : 'Unable to load agents',
        });
      });
    return true;
  }

  return true; // Keep channel open for async response
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

console.log('[GHOST] Background service worker initialized');
