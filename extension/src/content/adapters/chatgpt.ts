/**
 * ChatGPT DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class ChatGPTAdapter extends BasePlatformAdapter {
  platformId(): string {
    return 'chatgpt';
  }

  matches(url: string): boolean {
    return url.includes('chat.openai.com') || url.includes('chatgpt.com');
  }

  getMessageContainerSelectors(): string[] {
    return ['[class*="react-scroll-to-bottom"]', 'main .flex.flex-col', 'main'];
  }

  parseMessage(element: Element): ParsedMessage | null {
    const messageRoot = this.findClosest(element, ['[data-message-author-role]']);
    const role = messageRoot?.getAttribute('data-message-author-role');
    if (role !== 'assistant' && role !== 'user') return null;

    const content = this.extractText(
      this.findClosest(messageRoot, ['.markdown', '.whitespace-pre-wrap']) ?? messageRoot,
    );
    if (!content) return null;

    return {
      role: role === 'assistant' ? 'assistant' : 'human',
      content,
      timestamp: new Date(),
    };
  }
}
