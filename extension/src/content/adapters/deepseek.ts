/**
 * DeepSeek DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class DeepSeekAdapter extends BasePlatformAdapter {
  platformId(): string {
    return 'deepseek';
  }

  matches(url: string): boolean {
    return url.includes('chat.deepseek.com');
  }

  getMessageContainerSelectors(): string[] {
    return ['.chat-message-list', '[class*="conversation"]', 'main'];
  }

  parseMessage(element: Element): ParsedMessage | null {
    const userRoot = this.findClosest(element, ['.user-message', '[class*="user"]', '[class*="human"]']);
    const assistantRoot = this.findClosest(element, ['[class*="assistant"]', '[class*="bot"]']);
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
