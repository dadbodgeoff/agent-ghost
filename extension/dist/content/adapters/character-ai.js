/**
 * Character.AI DOM adapter.
 */
import { BasePlatformAdapter } from './base';
export class CharacterAIAdapter extends BasePlatformAdapter {
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