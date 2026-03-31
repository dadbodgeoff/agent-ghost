/**
 * DeepSeek DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class DeepSeekAdapter extends BasePlatformAdapter {
  matches(url: string): boolean {
    return url.includes('chat.deepseek.com');
  }

  getPlatformName(): string {
    return 'deepseek';
  }

  getMessageContainerSelector(): string {
    return '.chat-message-list';
  }

  parseMessage(element: Element): ParsedMessage | null {
    const content = element.textContent?.trim() || '';
    if (!content) return null;

    const isUser = element.classList.contains('user-message');

    return {
      role: isUser ? 'human' : 'assistant',
      content,
      timestamp: new Date(),
    };
  }
}
