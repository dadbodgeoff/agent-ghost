/**
 * ChatGPT DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class ChatGPTAdapter extends BasePlatformAdapter {
  readonly platform = 'chatgpt';

  matches(url: string): boolean {
    return url.includes('chat.openai.com') || url.includes('chatgpt.com');
  }

  getMessageContainerSelector(): string {
    return '[class*="react-scroll-to-bottom"]';
  }

  parseMessage(element: Element): ParsedMessage | null {
    const isAssistant = element.querySelector('[data-message-author-role="assistant"]');
    const isUser = element.querySelector('[data-message-author-role="user"]');

    if (!isAssistant && !isUser) return null;

    const content = element.textContent?.trim() || '';
    if (!content) return null;

    return {
      role: isAssistant ? 'assistant' : 'human',
      content,
      timestamp: new Date(),
    };
  }
}
