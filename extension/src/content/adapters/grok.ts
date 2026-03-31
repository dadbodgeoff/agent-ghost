/**
 * Grok DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class GrokAdapter extends BasePlatformAdapter {
  readonly platform = 'grok';

  matches(url: string): boolean {
    return url.includes('grok.x.ai');
  }

  getMessageContainerSelector(): string {
    return '.chat-container';
  }

  parseMessage(element: Element): ParsedMessage | null {
    const content = element.textContent?.trim() || '';
    if (!content) return null;

    const isUser = element.getAttribute('data-role') === 'user';

    return {
      role: isUser ? 'human' : 'assistant',
      content,
      timestamp: new Date(),
    };
  }
}
