/**
 * Background service worker — manages ITP emission, popup state, and session metadata.
 */

import { ITPEmitter } from './itp-emitter';

const LAST_PLATFORM_KEY = 'ghost-last-platform';
const SESSION_START_KEY = 'ghost-session-start';
const LAST_ACTIVITY_KEY = 'ghost-last-activity';

const emitter = new ITPEmitter();

function normalizePlatform(platform: unknown): string | null {
  if (typeof platform !== 'string' || platform.length === 0) return null;

  try {
    const url = new URL(platform);
    return url.hostname.replace(/^www\./, '');
  } catch {
    return platform;
  }
}

async function persistSessionMetadata(platform: unknown, sessionId: unknown): Promise<void> {
  const updates: Record<string, string | number> = {
    [LAST_ACTIVITY_KEY]: Date.now(),
  };

  const normalizedPlatform = normalizePlatform(platform);
  if (normalizedPlatform) {
    updates[LAST_PLATFORM_KEY] = normalizedPlatform;
  }

  if (typeof sessionId === 'string' && sessionId.length > 0) {
    const stored = await chrome.storage.local.get(SESSION_START_KEY);
    if (typeof stored[SESSION_START_KEY] !== 'number') {
      updates[SESSION_START_KEY] = Date.now();
    }
  }

  await chrome.storage.local.set(updates);
}

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
    void persistSessionMetadata(message.platform, message.sessionId);
    sendResponse({ ok: true });
    return true;
  }

  if (message.type === 'SESSION_START') {
    emitter.emit({
      eventType: 'SessionStart',
      platform: message.platform,
      timestamp: new Date().toISOString(),
      sessionId: message.sessionId,
    });
    void chrome.storage.local.set({
      [SESSION_START_KEY]: Date.now(),
      [LAST_ACTIVITY_KEY]: Date.now(),
      ...(normalizePlatform(message.platform)
        ? { [LAST_PLATFORM_KEY]: normalizePlatform(message.platform) as string }
        : {}),
    });
    sendResponse({ ok: true });
    return true;
  }

  if (message.type === 'GET_SCORE') {
    sendResponse({ score: emitter.getLatestScore() });
    return true;
  }

  if (message.type === 'GET_POPUP_STATE') {
    void chrome.storage.local
      .get([LAST_PLATFORM_KEY, SESSION_START_KEY])
      .then((stored) => {
        sendResponse({
          score: emitter.getLatestScore(),
          platform: typeof stored[LAST_PLATFORM_KEY] === 'string' ? stored[LAST_PLATFORM_KEY] : undefined,
          sessionStart: typeof stored[SESSION_START_KEY] === 'number' ? stored[SESSION_START_KEY] : undefined,
        });
      })
      .catch(() => {
        sendResponse({ score: emitter.getLatestScore() });
      });
    return true;
  }

  return true;
});

setInterval(() => {
  emitter.refreshScore();
}, 30_000);

console.log('[GHOST] Background service worker initialized');
