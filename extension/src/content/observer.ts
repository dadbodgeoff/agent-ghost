/**
 * Content script — observes DOM for new messages and emits to background.
 */

import { BasePlatformAdapter } from './adapters/base';
import { ChatGPTAdapter } from './adapters/chatgpt';
import { ClaudeAdapter } from './adapters/claude';
import { CharacterAIAdapter } from './adapters/character-ai';
import { GeminiAdapter } from './adapters/gemini';
import { DeepSeekAdapter } from './adapters/deepseek';
import { GrokAdapter } from './adapters/grok';

const adapters: BasePlatformAdapter[] = [
  new ChatGPTAdapter(),
  new ClaudeAdapter(),
  new CharacterAIAdapter(),
  new GeminiAdapter(),
  new DeepSeekAdapter(),
  new GrokAdapter(),
];

let initialized = false;

function init(): void {
  if (initialized) {
    return;
  }

  const url = window.location.href;
  const adapter = adapters.find(a => a.matches(url));

  if (!adapter) {
    console.log('[GHOST] No adapter for this page');
    return;
  }

  initialized = true;
  const platform = window.location.hostname;
  const sessionId = generateSessionId();

  console.log(`[GHOST] Using adapter for: ${platform}`);

  // Notify session start
  void chrome.runtime.sendMessage({
    type: 'SESSION_START',
    platform,
    sessionId,
  }).catch(() => {});

  // Observe new messages
  adapter.observeNewMessages(async (msg) => {
    const contentHash = await adapter.hashContent(msg.content);
    void chrome.runtime.sendMessage({
      type: 'NEW_MESSAGE',
      platform,
      role: msg.role,
      contentHash,
      sessionId,
    }).catch(() => {});
  });
}

function generateSessionId(): string {
  const stored = sessionStorage.getItem('ghost-session-id');
  if (stored) return stored;
  const id = crypto.randomUUID();
  sessionStorage.setItem('ghost-session-id', id);
  return id;
}

// Initialize when DOM is ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
