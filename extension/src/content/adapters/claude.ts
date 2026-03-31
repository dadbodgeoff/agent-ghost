/**
 * Claude.ai DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class ClaudeAdapter extends BasePlatformAdapter {
  platformId(): string {
    return 'claude';
  }

  matches(url: string): boolean {
    return url.includes('claude.ai');
  }

  getMessageContainerSelectors(): string[] {
    return ['[class*="conversation-content"]', '[class*="conversation"]', 'main'];
  }

  parseMessage(element: Element): ParsedMessage | null {
    const humanRoot = this.findClosest(element, ['[class*="human-message"]', '[class*="user"]']);
    const assistantRoot = this.findClosest(element, ['[class*="assistant-message"]', '[class*="claude"]']);
    if (!humanRoot && !assistantRoot) return null;

    const content = this.extractText(assistantRoot ?? humanRoot);
    if (!content) return null;

    return {
      role: assistantRoot ? 'assistant' : 'human',
      content,
      timestamp: new Date(),
    };
  }
}
