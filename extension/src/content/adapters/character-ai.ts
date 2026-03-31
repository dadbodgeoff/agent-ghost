/**
 * Character.AI DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base.js';

export class CharacterAIAdapter extends BasePlatformAdapter {
  matches(url: string): boolean {
    return url.includes('character.ai');
  }

  getMessageContainerSelector(): string {
    return '[class*="chat-messages"]';
  }

  parseMessage(element: Element): ParsedMessage | null {
    const isHuman = element.classList.contains('human') ||
                    element.querySelector('[class*="human"]') !== null;

    const content = element.textContent?.trim() || '';
    if (!content) return null;

    return {
      role: isHuman ? 'human' : 'assistant',
      content,
      timestamp: new Date(),
    };
  }
}
