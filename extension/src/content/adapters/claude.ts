/**
 * Claude.ai DOM adapter.
 */

import { BasePlatformAdapter, ParsedMessage } from './base';

export class ClaudeAdapter extends BasePlatformAdapter {
  readonly platformName = 'claude';

  matches(url: string): boolean {
    return url.includes('claude.ai');
  }

  getMessageContainerSelector(): string {
    return '[class*="conversation-content"]';
  }

  parseMessage(element: Element): ParsedMessage | null {
    const isHuman = element.querySelector('[class*="human-message"]');
    const isAssistant = element.querySelector('[class*="assistant-message"]');

    if (!isHuman && !isAssistant) return null;

    const content = element.textContent?.trim() || '';
    if (!content) return null;

    return {
      role: isAssistant ? 'assistant' : 'human',
      content,
      timestamp: new Date(),
    };
  }
}
