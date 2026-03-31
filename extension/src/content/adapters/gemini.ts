/**
 * Gemini DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class GeminiAdapter extends BasePlatformAdapter {
  platformId(): string {
    return 'gemini';
  }

  matches(url: string): boolean {
    return url.includes('gemini.google.com');
  }

  getMessageContainerSelectors(): string[] {
    return ['.conversation-container', 'chat-window', '[class*="conversation"]', 'main'];
  }

  parseMessage(element: Element): ParsedMessage | null {
    const userRoot = this.findClosest(element, ['[class*="query"]', '[class*="user"]']);
    const modelRoot = this.findClosest(element, ['[class*="response"]', '[class*="model"]']);
    if (!userRoot && !modelRoot) return null;

    const content = this.extractText(modelRoot ?? userRoot);
    if (!content) return null;

    return {
      role: modelRoot ? 'assistant' : 'human',
      content,
      timestamp: new Date(),
    };
  }
}
