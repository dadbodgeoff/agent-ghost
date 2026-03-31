/**
 * Content script — observes DOM for new messages and emits to background.
 */

import { ChatGPTAdapter } from './adapters/chatgpt.js';
import { ClaudeAdapter } from './adapters/claude.js';
import { CharacterAIAdapter } from './adapters/character-ai.js';
import { GeminiAdapter } from './adapters/gemini.js';
import { DeepSeekAdapter } from './adapters/deepseek.js';
import { GrokAdapter } from './adapters/grok.js';

const adapters = [
  new ChatGPTAdapter(),
  new ClaudeAdapter(),
  new CharacterAIAdapter(),
  new GeminiAdapter(),
  new DeepSeekAdapter(),
  new GrokAdapter(),
];

function init() {
  const url = window.location.href;
  const adapter = adapters.find((a) => a.constructor.matches(url));
  const platform = new URL(url).hostname;
  const sessionId = generateSessionId();

  if (!adapter) {
    console.log('[GHOST] No adapter for this page');
    return;
  }

  console.log(`[GHOST] Using adapter for: ${url}`);

  chrome.runtime.sendMessage({
    type: 'SESSION_START',
    platform,
    sessionId,
  });

  const container = adapter.getMessageContainer();
  if (!container) {
    return;
  }

  const observer = new MutationObserver(async (mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (!(node instanceof Element)) continue;
        const msg = adapter.parseMessage(node);
        if (!msg) continue;
        const contentHash = await hashContent(msg.content);
        chrome.runtime.sendMessage({
          type: 'NEW_MESSAGE',
          platform,
          role: msg.role,
          contentHash,
          sessionId,
        });
      }
    }
  });

  observer.observe(container, { childList: true, subtree: true });
}

function generateSessionId() {
  const stored = sessionStorage.getItem('ghost-session-id');
  if (stored) return stored;
  const id = crypto.randomUUID();
  sessionStorage.setItem('ghost-session-id', id);
  return id;
}

async function hashContent(content) {
  const encoder = new TextEncoder();
  const data = encoder.encode(content);
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map((b) => b.toString(16).padStart(2, '0')).join('');
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
