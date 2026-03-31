/**
 * ChatGPT DOM adapter.
 */
import { BasePlatformAdapter } from './base.js';
export class ChatGPTAdapter extends BasePlatformAdapter {
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
