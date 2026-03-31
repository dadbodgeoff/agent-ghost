/**
 * Background service worker — manages ITP emission and native messaging.
 */

import { initAuthSync } from './auth-sync';
import { ITPEmitter } from './itp-emitter';

const emitter = new ITPEmitter();
const DEFAULT_STORAGE_STATE = {
  'ghost-last-sync': 0,
  'ghost-last-platform': '',
  'ghost-active-session-id': '',
};

void initAuthSync().catch(() => {});

chrome.runtime.onInstalled.addListener(() => {
  void chrome.storage.local.set(DEFAULT_STORAGE_STATE);
});

function normalizePlatformLabel(platform: string | undefined): string {
  if (!platform) return 'Unknown';

  try {
    const host = new URL(platform).hostname.replace(/^www\./, '');
    switch (host) {
      case 'chatgpt.com':
      case 'chat.openai.com':
        return 'ChatGPT';
      case 'claude.ai':
        return 'Claude';
      case 'character.ai':
        return 'Character.AI';
      case 'gemini.google.com':
        return 'Gemini';
      case 'chat.deepseek.com':
        return 'DeepSeek';
      case 'grok.x.ai':
        return 'Grok';
      default:
        return host;
    }
  } catch {
    return platform;
  }
}

async function persistActivity(platform: string | undefined, sessionId: string | undefined): Promise<void> {
  await chrome.storage.local.set({
    'ghost-last-sync': Date.now(),
    'ghost-last-platform': normalizePlatformLabel(platform),
    ...(sessionId ? { 'ghost-active-session-id': sessionId } : {}),
  });
}

// Listen for messages from content scripts
chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === 'NEW_MESSAGE') {
    void persistActivity(message.platform, message.sessionId);
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
    void persistActivity(message.platform, message.sessionId);
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
    void chrome.storage.local
      .get(['ghost-last-platform', 'ghost-active-session-id', 'ghost-last-sync'])
      .then((stored) => {
        sendResponse({
          ...emitter.getLatestScore(),
          platform: typeof stored['ghost-last-platform'] === 'string' ? stored['ghost-last-platform'] : '',
          sessionId:
            typeof stored['ghost-active-session-id'] === 'string'
              ? stored['ghost-active-session-id']
              : '',
          lastSync:
            typeof stored['ghost-last-sync'] === 'number' ? stored['ghost-last-sync'] : 0,
        });
      })
      .catch(() => {
        sendResponse({
          ...emitter.getLatestScore(),
          platform: '',
          sessionId: '',
          lastSync: 0,
        });
      });
    return true;
  }

  return false;
});

// Periodic score refresh
setInterval(() => {
  emitter.refreshScore();
}, 30_000);

console.log('[GHOST] Background service worker initialized');
