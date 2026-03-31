/**
 * DeepSeek DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class DeepSeekAdapter extends BasePlatformAdapter {
  readonly platform = 'deepseek';

  matches(url: string): boolean {
    return url.includes('chat.deepseek.com');
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
