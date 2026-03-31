/**
 * Grok DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class GrokAdapter extends BasePlatformAdapter {
  platformId(): string {
    return 'grok';
  }

  matches(url: string): boolean {
    return url.includes('grok.x.ai');
  }

  getMessageContainerSelectors(): string[] {
    return ['.chat-container', '[class*="message-list"]', '[class*="conversation"]', 'main'];
  }

  parseMessage(element: Element): ParsedMessage | null {
    const userRoot = this.findClosest(element, ['[data-role="user"]', '[class*="user"]', '[class*="human"]']);
    const assistantRoot = this.findClosest(element, ['[data-role="assistant"]', '[class*="assistant"]', '[class*="grok"]']);
    if (!userRoot && !assistantRoot) return null;

    const content = this.extractText(assistantRoot ?? userRoot);
    if (!content) return null;

    return {
      role: userRoot ? 'human' : 'assistant',
      content,
      timestamp: new Date(),
    };
  }
}
