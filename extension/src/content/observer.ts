/**
 * Content script — observes DOM for new messages and emits to background.
 */

import { BasePlatformAdapter } from './adapters/base.js';
import { ChatGPTAdapter } from './adapters/chatgpt.js';
import { ClaudeAdapter } from './adapters/claude.js';
import { CharacterAIAdapter } from './adapters/character-ai.js';
import { GeminiAdapter } from './adapters/gemini.js';
import { DeepSeekAdapter } from './adapters/deepseek.js';
import { GrokAdapter } from './adapters/grok.js';

const adapters: BasePlatformAdapter[] = [
  new ChatGPTAdapter(),
  new ClaudeAdapter(),
  new CharacterAIAdapter(),
  new GeminiAdapter(),
  new DeepSeekAdapter(),
  new GrokAdapter(),
];

let activeObserver: MutationObserver | null = null;
let lastInitializedUrl: string | null = null;

function init(): void {
  const url = window.location.href;
  const adapter = adapters.find(a => a.matches(url));

  if (!adapter) {
    console.log('[GHOST] No adapter for this page');
    return;
  }

  console.log(`[GHOST] Using adapter for: ${url}`);

  activeObserver?.disconnect();

  const sessionId = generateSessionId();

  if (lastInitializedUrl !== url) {
    chrome.runtime.sendMessage({
      type: 'SESSION_START',
      platform: url,
      sessionId,
    });
    lastInitializedUrl = url;
  }

  // Observe new messages
  activeObserver = adapter.observeNewMessages(async (msg) => {
    const contentHash = await adapter.hashContent(msg.content);
    chrome.runtime.sendMessage({
      type: 'NEW_MESSAGE',
      platform: url,
      role: msg.role,
      contentHash,
      sessionId,
    });
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

let lastUrl = window.location.href;
setInterval(() => {
  if (window.location.href !== lastUrl) {
    lastUrl = window.location.href;
    lastInitializedUrl = null;
    init();
  }
}, 1000);
