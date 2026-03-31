/**
 * Base platform adapter — abstract class for DOM observation.
 */
class BasePlatformAdapter {
    observeNewMessages(callback) {
        const selector = this.getMessageContainerSelector();
        const container = document.querySelector(selector);
        const observer = new MutationObserver((mutations) => {
            for (const mutation of mutations) {
                for (const node of mutation.addedNodes) {
                    if (node instanceof Element) {
                        const msg = this.parseMessage(node);
                        if (msg) {
                            callback(msg);
                        }
                    }
                }
            }
        });
        if (container) {
            observer.observe(container, { childList: true, subtree: true });
        }
        return observer;
    }
    /** SHA-256 hash of content for privacy. */
    async hashContent(content) {
        const encoder = new TextEncoder();
        const data = encoder.encode(content);
        const hashBuffer = await crypto.subtle.digest('SHA-256', data);
        const hashArray = Array.from(new Uint8Array(hashBuffer));
        return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
    }
}
//# sourceMappingURL=base.js.map
/**
 * ChatGPT DOM adapter.
 */

class ChatGPTAdapter extends BasePlatformAdapter {
    matches(url) {
        return url.includes('chat.openai.com') || url.includes('chatgpt.com');
    }
    getMessageContainerSelector() {
        return '[class*="react-scroll-to-bottom"]';
    }
    parseMessage(element) {
        const isAssistant = element.querySelector('[data-message-author-role="assistant"]');
        const isUser = element.querySelector('[data-message-author-role="user"]');
        if (!isAssistant && !isUser)
            return null;
        const content = element.textContent?.trim() || '';
        if (!content)
            return null;
        return {
            role: isAssistant ? 'assistant' : 'human',
            content,
            timestamp: new Date(),
        };
    }
}
//# sourceMappingURL=chatgpt.js.map

/**
 * Claude.ai DOM adapter.
 */

class ClaudeAdapter extends BasePlatformAdapter {
    matches(url) {
        return url.includes('claude.ai');
    }
    getMessageContainerSelector() {
        return '[class*="conversation-content"]';
    }
    parseMessage(element) {
        const isHuman = element.querySelector('[class*="human-message"]');
        const isAssistant = element.querySelector('[class*="assistant-message"]');
        if (!isHuman && !isAssistant)
            return null;
        const content = element.textContent?.trim() || '';
        if (!content)
            return null;
        return {
            role: isAssistant ? 'assistant' : 'human',
            content,
            timestamp: new Date(),
        };
    }
}
//# sourceMappingURL=claude.js.map

/**
 * Character.AI DOM adapter.
 */

class CharacterAIAdapter extends BasePlatformAdapter {
    matches(url) {
        return url.includes('character.ai');
    }
    getMessageContainerSelector() {
        return '[class*="chat-messages"]';
    }
    parseMessage(element) {
        const isHuman = element.classList.contains('human') ||
            element.querySelector('[class*="human"]') !== null;
        const content = element.textContent?.trim() || '';
        if (!content)
            return null;
        return {
            role: isHuman ? 'human' : 'assistant',
            content,
            timestamp: new Date(),
        };
    }
}
//# sourceMappingURL=character-ai.js.map

/**
 * Gemini DOM adapter.
 */

class GeminiAdapter extends BasePlatformAdapter {
    matches(url) {
        return url.includes('gemini.google.com');
    }
    getMessageContainerSelector() {
        return '.conversation-container';
    }
    parseMessage(element) {
        const isUser = element.querySelector('[class*="query"]') !== null;
        const isModel = element.querySelector('[class*="response"]') !== null;
        if (!isUser && !isModel)
            return null;
        const content = element.textContent?.trim() || '';
        if (!content)
            return null;
        return {
            role: isModel ? 'assistant' : 'human',
            content,
            timestamp: new Date(),
        };
    }
}
//# sourceMappingURL=gemini.js.map

/**
 * DeepSeek DOM adapter.
 */

class DeepSeekAdapter extends BasePlatformAdapter {
    matches(url) {
        return url.includes('chat.deepseek.com');
    }
    getMessageContainerSelector() {
        return '.chat-message-list';
    }
    parseMessage(element) {
        const content = element.textContent?.trim() || '';
        if (!content)
            return null;
        const isUser = element.classList.contains('user-message');
        return {
            role: isUser ? 'human' : 'assistant',
            content,
            timestamp: new Date(),
        };
    }
}
//# sourceMappingURL=deepseek.js.map

/**
 * Grok DOM adapter.
 */

class GrokAdapter extends BasePlatformAdapter {
    matches(url) {
        return url.includes('grok.x.ai');
    }
    getMessageContainerSelector() {
        return '.chat-container';
    }
    parseMessage(element) {
        const content = element.textContent?.trim() || '';
        if (!content)
            return null;
        const isUser = element.getAttribute('data-role') === 'user';
        return {
            role: isUser ? 'human' : 'assistant',
            content,
            timestamp: new Date(),
        };
    }
}
//# sourceMappingURL=grok.js.map
/**
 * Content script — observes DOM for new messages and emits to background.
 */






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
    const adapter = adapters.find(a => a.matches(url));
    if (!adapter) {
        console.log('[GHOST] No adapter for this page');
        return;
    }
    console.log(`[GHOST] Using adapter for: ${url}`);
    // Notify session start
    chrome.runtime.sendMessage({
        type: 'SESSION_START',
        platform: url,
        sessionId: generateSessionId(),
    });
    // Observe new messages
    adapter.observeNewMessages(async (msg) => {
        const contentHash = await adapter.hashContent(msg.content);
        chrome.runtime.sendMessage({
            type: 'NEW_MESSAGE',
            platform: url,
            role: msg.role,
            contentHash,
            sessionId: generateSessionId(),
        });
    });
}
function generateSessionId() {
    const stored = sessionStorage.getItem('ghost-session-id');
    if (stored)
        return stored;
    const id = crypto.randomUUID();
    sessionStorage.setItem('ghost-session-id', id);
    return id;
}
// Initialize when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
}
else {
    init();
}
//# sourceMappingURL=observer.js.map

