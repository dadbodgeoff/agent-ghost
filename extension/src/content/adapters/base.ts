/**
 * Base platform adapter — abstract class for DOM observation.
 */

export interface ParsedMessage {
  role: 'human' | 'assistant';
  content: string;
  timestamp: Date;
}

export abstract class BasePlatformAdapter {
  abstract matches(url: string): boolean;
  abstract getPlatformName(): string;
  abstract getMessageContainerSelector(): string;
  abstract parseMessage(element: Element): ParsedMessage | null;

  observeNewMessages(callback: (msg: ParsedMessage) => void): MutationObserver {
    const selector = this.getMessageContainerSelector();
    const emitFromElement = (element: Element) => {
      const parsed = this.parseMessage(element);
      if (parsed) {
        callback(parsed);
      }

      const descendants = element.querySelectorAll('*');
      for (const descendant of descendants) {
        const nested = this.parseMessage(descendant);
        if (nested) {
          callback(nested);
        }
      }
    };

    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        for (const node of mutation.addedNodes) {
          if (node instanceof Element) {
            emitFromElement(node);
          }
        }
      }
    });

    const attachToContainer = () => {
      const container = document.querySelector(selector);
      if (!container) {
        return false;
      }

      observer.observe(container, { childList: true, subtree: true });
      return true;
    };

    if (!attachToContainer() && document.body) {
      const bootstrapObserver = new MutationObserver(() => {
        if (attachToContainer()) {
          bootstrapObserver.disconnect();
        }
      });

      bootstrapObserver.observe(document.body, { childList: true, subtree: true });
    }

    return observer;
  }

  /** SHA-256 hash of content for privacy. */
  async hashContent(content: string): Promise<string> {
    const encoder = new TextEncoder();
    const data = encoder.encode(content);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
  }
}
