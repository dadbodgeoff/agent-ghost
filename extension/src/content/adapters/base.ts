/**
 * Base platform adapter — abstract class for DOM observation.
 */

export interface ParsedMessage {
  role: 'human' | 'assistant';
  content: string;
  timestamp: Date;
}

export abstract class BasePlatformAdapter {
  abstract platformId(): string;
  abstract matches(url: string): boolean;
  abstract getMessageContainerSelectors(): string[];
  abstract parseMessage(element: Element): ParsedMessage | null;

  findMessageContainer(): Element | null {
    for (const selector of this.getMessageContainerSelectors()) {
      const container = document.querySelector(selector);
      if (container) {
        return container;
      }
    }
    return null;
  }

  observeNewMessages(callback: (msg: ParsedMessage) => void): () => void {
    let containerObserver: MutationObserver | null = null;
    let rootObserver: MutationObserver | null = null;

    const visitNode = (node: Node) => {
      if (!(node instanceof Element)) {
        return;
      }

      const parsed = this.parseMessage(node);
      if (parsed) {
        callback(parsed);
      }

      for (const descendant of node.querySelectorAll('*')) {
        const nested = this.parseMessage(descendant);
        if (nested) {
          callback(nested);
        }
      }
    };

    const attachToContainer = (container: Element) => {
      if (containerObserver) {
        containerObserver.disconnect();
      }

      containerObserver = new MutationObserver((mutations) => {
        for (const mutation of mutations) {
          for (const node of mutation.addedNodes) {
            visitNode(node);
          }
        }
      });

      containerObserver.observe(container, { childList: true, subtree: true });
    };

    const existingContainer = this.findMessageContainer();
    if (existingContainer) {
      attachToContainer(existingContainer);
    } else {
      rootObserver = new MutationObserver(() => {
        const container = this.findMessageContainer();
        if (!container) {
          return;
        }
        attachToContainer(container);
        rootObserver?.disconnect();
        rootObserver = null;
      });

      const root = document.body ?? document.documentElement;
      if (root) {
        rootObserver.observe(root, { childList: true, subtree: true });
      }
    }

    return () => {
      containerObserver?.disconnect();
      rootObserver?.disconnect();
    };
  }

  /** SHA-256 hash of content for privacy. */
  async hashContent(content: string): Promise<string> {
    const encoder = new TextEncoder();
    const data = encoder.encode(content);
    const hashBuffer = await crypto.subtle.digest('SHA-256', data);
    const hashArray = Array.from(new Uint8Array(hashBuffer));
    return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
  }

  protected findClosest(
    element: Element | null | undefined,
    selectors: string[],
  ): Element | null {
    if (!element) {
      return null;
    }

    for (const selector of selectors) {
      if (element.matches(selector)) {
        return element;
      }

      const nested = element.querySelector(selector);
      if (nested) {
        return nested;
      }
    }

    return null;
  }

  protected extractText(element: Element | null): string {
    return element?.textContent?.trim() || '';
  }
}
