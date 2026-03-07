/**
 * DeepSeek DOM adapter.
 */
import { BasePlatformAdapter } from './base';
export class DeepSeekAdapter extends BasePlatformAdapter {
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