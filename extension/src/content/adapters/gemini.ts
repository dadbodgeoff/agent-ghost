/**
 * Gemini DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class GeminiAdapter extends BasePlatformAdapter {
  matches(url: string): boolean {
    return url.includes('gemini.google.com');
  }

  getPlatformName(): string {
    return 'gemini';
  }

  getMessageContainerSelector(): string {
    return '.conversation-container';
  }

  parseMessage(element: Element): ParsedMessage | null {
    const isUser = element.querySelector('[class*="query"]') !== null;
    const isModel = element.querySelector('[class*="response"]') !== null;

    if (!isUser && !isModel) return null;

    const content = element.textContent?.trim() || '';
    if (!content) return null;

    return {
      role: isModel ? 'assistant' : 'human',
      content,
      timestamp: new Date(),
    };
  }
}
