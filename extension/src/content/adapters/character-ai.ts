/**
 * Character.AI DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class CharacterAIAdapter extends BasePlatformAdapter {
  platformId(): string {
    return 'character_ai';
  }

  matches(url: string): boolean {
    return url.includes('character.ai');
  }

  getMessageContainerSelectors(): string[] {
    return ['[class*="chat-messages"]', '[class*="msg-list"]', 'main'];
  }

  parseMessage(element: Element): ParsedMessage | null {
    const humanRoot = this.findClosest(element, ['[class*="human"]', '[class*="user"]']);
    const assistantRoot = this.findClosest(element, ['[class*="char"]', '[class*="bot"]']);
    if (!humanRoot && !assistantRoot) return null;

    const content = this.extractText(assistantRoot ?? humanRoot);
    if (!content) return null;

    return {
      role: humanRoot ? 'human' : 'assistant',
      content,
      timestamp: new Date(),
    };
  }
}
