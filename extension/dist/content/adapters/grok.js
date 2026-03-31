/**
 * Grok DOM adapter.
 */
import { BasePlatformAdapter } from './base.js';
export class GrokAdapter extends BasePlatformAdapter {
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
