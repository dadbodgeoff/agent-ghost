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
let activeObserver: MutationObserver | null = null;

function sendRuntimeMessage(message: Record<string, unknown>): void {
  chrome.runtime.sendMessage(message, () => {
    void chrome.runtime.lastError;
  });
}

function init(): void {
  if (initialized) return;
  initialized = true;

  const url = window.location.href;
  const sessionId = generateSessionId();
  const adapter = adapters.find((a) => a.matches(url));

  if (!adapter) {
    console.log('[GHOST] No adapter for this page');
    return;
  }

  console.log(`[GHOST] Using adapter for: ${url}`);

  // Notify session start
  sendRuntimeMessage({
    type: 'SESSION_START',
    platform: url,
    sessionId,
  });

  // Observe new messages
  activeObserver = adapter.observeNewMessages(async (msg) => {
    const contentHash = await adapter.hashContent(msg.content);
    sendRuntimeMessage({
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

window.addEventListener('beforeunload', () => {
  activeObserver?.disconnect();
  activeObserver = null;
});
