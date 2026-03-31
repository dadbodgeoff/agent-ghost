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

function sendMessage(message: Record<string, unknown>): void {
  if (!chrome.runtime?.id) return;
  void chrome.runtime.sendMessage(message).catch(() => {
    // Non-fatal during tab teardown or extension reloads.
  });
}

function init(): void {
  const url = window.location.href;
  const adapter = adapters.find(a => a.matches(url));

  if (!adapter) {
    return;
  }
  const sessionId = generateSessionId();
  const platform = window.location.hostname;

  // Notify session start
  sendMessage({
    type: 'SESSION_START',
    platform,
    sessionId,
  });

  // Observe new messages
  adapter.observeNewMessages(async (msg) => {
    const contentHash = await adapter.hashContent(msg.content);
    sendMessage({
      type: 'NEW_MESSAGE',
      platform,
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
