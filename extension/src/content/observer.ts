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

let activeAdapter: BasePlatformAdapter | null = null;
let messageObserver: MutationObserver | null = null;
let urlObserver: MutationObserver | null = null;
let sessionId: string | null = null;
let messageCount = 0;
let lastUrl = window.location.href;
let containerPollId: number | null = null;
let containerPollTimeoutId: number | null = null;

function init(): void {
  startObserving();
  observeUrlChanges();
  window.addEventListener('beforeunload', emitSessionEnd);
}

function startObserving(): void {
  cleanupObservers();

  const url = window.location.href;
  activeAdapter = adapters.find(a => a.matches(url)) ?? null;

  if (!activeAdapter) {
    console.log('[GHOST] No adapter for this page');
    return;
  }

  sessionId = generateSessionId(true);
  messageCount = 0;

  console.log(`[GHOST] Using adapter for: ${activeAdapter.platformName}`);

  chrome.runtime.sendMessage({
    type: 'SESSION_START',
    platform: activeAdapter.platformName,
    sessionId,
  });

  waitForContainer(activeAdapter);
}

function waitForContainer(adapter: BasePlatformAdapter): void {
  const attach = () => {
    const container = adapter.getMessageContainer();
    if (!container) {
      return false;
    }

    if (containerPollId != null) {
      window.clearInterval(containerPollId);
      containerPollId = null;
    }
    if (containerPollTimeoutId != null) {
      window.clearTimeout(containerPollTimeoutId);
      containerPollTimeoutId = null;
    }

    processExistingMessages(adapter, container);
    messageObserver = adapter.observeNewMessages((msg) => {
      void emitMessage(adapter, msg);
    });
    return true;
  };

  if (attach()) {
    return;
  }

  containerPollId = window.setInterval(() => {
    attach();
  }, 1000);
  containerPollTimeoutId = window.setTimeout(() => {
    if (containerPollId != null) {
      window.clearInterval(containerPollId);
      containerPollId = null;
    }
  }, 30_000);
}

function processExistingMessages(adapter: BasePlatformAdapter, container: Element): void {
  for (const element of adapter.getExistingMessages(container)) {
    const msg = adapter.parseMessage(element);
    if (msg) {
      void emitMessage(adapter, msg);
    }
  }
}

async function emitMessage(adapter: BasePlatformAdapter, msg: { role: 'human' | 'assistant'; content: string }): Promise<void> {
  if (!sessionId) {
    sessionId = generateSessionId();
  }

  messageCount += 1;
  const contentHash = await adapter.hashContent(msg.content);
  chrome.runtime.sendMessage({
    type: 'NEW_MESSAGE',
    platform: adapter.platformName,
    role: msg.role,
    contentHash,
    sessionId,
  });
}

function observeUrlChanges(): void {
  if (urlObserver || !document.body) {
    return;
  }

  urlObserver = new MutationObserver(() => {
    if (window.location.href === lastUrl) {
      return;
    }

    emitSessionEnd();
    lastUrl = window.location.href;
    startObserving();
  });
  urlObserver.observe(document.body, { childList: true, subtree: true });
}

function emitSessionEnd(): void {
  if (!sessionId || !activeAdapter) {
    return;
  }

  chrome.runtime.sendMessage({
    type: 'SESSION_END',
    platform: activeAdapter.platformName,
    sessionId,
    messageCount,
  });
  sessionStorage.removeItem('ghost-session-id');
  sessionId = null;
  messageCount = 0;
}

function cleanupObservers(): void {
  if (messageObserver) {
    messageObserver.disconnect();
    messageObserver = null;
  }
  if (containerPollId != null) {
    window.clearInterval(containerPollId);
    containerPollId = null;
  }
  if (containerPollTimeoutId != null) {
    window.clearTimeout(containerPollTimeoutId);
    containerPollTimeoutId = null;
  }
}

function generateSessionId(forceNew = false): string {
  if (forceNew) {
    const freshId = crypto.randomUUID();
    sessionStorage.setItem('ghost-session-id', freshId);
    return freshId;
  }
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
