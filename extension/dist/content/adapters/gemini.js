/**
 * Gemini DOM adapter.
 */
import { BasePlatformAdapter } from './base';
export class GeminiAdapter extends BasePlatformAdapter {
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